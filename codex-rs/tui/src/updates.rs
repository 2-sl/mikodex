#![cfg(not(debug_assertions))]

use crate::legacy_core::config::Config;
use crate::npm_registry;
use crate::npm_registry::NpmPackageInfo;
use crate::update_action;
use crate::update_action::UpdateAction;
use crate::update_versions::extract_version_from_latest_tag;
use crate::update_versions::is_newer;
use crate::update_versions::is_source_build_version;
use crate::updates_cache::VersionInfo;
use crate::updates_cache::read_version_info;
use crate::updates_cache::version_filepath;
use chrono::Duration;
use chrono::Utc;
use codex_http_client::ClientRouteClass;
use codex_http_client::HttpClientFactory;
use codex_http_client::RouteAwareClientPool;
use codex_install_context::InstallContext;
use codex_login::default_client::default_headers;
use serde::Deserialize;
use std::path::Path;

use crate::version::CODEX_CLI_VERSION;

pub(crate) use crate::updates_cache::dismiss_version;

pub fn get_upgrade_version(config: &Config) -> Option<String> {
    if !config.check_for_update_on_startup || is_source_build_version(CODEX_CLI_VERSION) {
        return None;
    }

    let action = update_action::get_update_action();
    let version_file = version_filepath(config);
    let info = read_version_info(&version_file).ok();

    let is_mikodex_rolling = matches!(action, Some(UpdateAction::MikodexRolling));
    let should_refresh = match &info {
        None => true,
        Some(info) => {
            info.last_checked_at < Utc::now() - Duration::hours(20)
                || (is_mikodex_rolling && info.latest_asset_sha256.is_none())
        }
    };
    if should_refresh {
        let http_client_factory = config.http_client_factory();
        // Refresh the cached latest version in the background so TUI startup
        // isn’t blocked by a network call. The UI reads the previously cached
        // value (if any) for this run; the next run shows the banner if needed.
        tokio::spawn(async move {
            check_for_update(&version_file, action, http_client_factory)
                .await
                .inspect_err(|e| tracing::error!("Failed to update version: {e}"))
        });
    }

    info.and_then(|info| {
        let update_available = if is_mikodex_rolling {
            info.latest_asset_sha256
                .as_deref()
                .is_some_and(|digest| !mikodex_rolling_receipt_matches(digest))
        } else {
            is_newer(&info.latest_version, CODEX_CLI_VERSION).unwrap_or(false)
        };
        update_available.then_some(info.latest_version)
    })
}

// We use the latest version from the cask if installation is via homebrew - homebrew does not immediately pick up the latest release and can lag behind.
const HOMEBREW_CASK_API_URL: &str = "https://formulae.brew.sh/api/cask/mikodex.json";
const LATEST_RELEASE_URL: &str = "https://api.github.com/repos/openai/codex/releases/latest";
const MIKODEX_ROLLING_RELEASE_URL: &str =
    "https://api.github.com/repos/2-sl/mikodex/releases/tags/mikodex-rolling";
const MIKODEX_ROLLING_TAG: &str = "mikodex-rolling";
const MIKODEX_ROLLING_PACKAGE_ASSET: &str = "mikodex-windows-x64.zip";

#[derive(Deserialize, Debug, Clone)]
struct ReleaseInfo {
    tag_name: String,
}

#[derive(Deserialize, Debug, Clone)]
struct HomebrewCaskInfo {
    version: String,
}

#[derive(Deserialize)]
struct MikodexRollingRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<MikodexRollingAsset>,
}

#[derive(Deserialize)]
struct MikodexRollingAsset {
    name: String,
    digest: Option<String>,
}

async fn check_for_update(
    version_file: &Path,
    action: Option<UpdateAction>,
    http_client_factory: HttpClientFactory,
) -> anyhow::Result<()> {
    let client_pool = RouteAwareClientPool::with_chatgpt_cloudflare_cookies(
        http_client_factory,
        ClientRouteClass::Other,
    )
    .with_legacy_custom_ca_fallback();
    let (latest_version, latest_asset_sha256) = match action {
        Some(UpdateAction::MikodexRolling) => {
            let digest = fetch_latest_mikodex_rolling_digest(&client_pool).await?;
            (
                format!("{CODEX_CLI_VERSION} (rolling {})", &digest[..12]),
                Some(digest),
            )
        }
        Some(UpdateAction::Daemon(_)) => return Ok(()),
        Some(UpdateAction::BrewUpgrade) => {
            let HomebrewCaskInfo { version } = client_pool
                .get(HOMEBREW_CASK_API_URL)
                .headers(default_headers())
                .send()
                .await?
                .error_for_status()?
                .json::<HomebrewCaskInfo>()
                .await?;
            (version, None)
        }
        Some(UpdateAction::NpmGlobalLatest)
        | Some(UpdateAction::BunGlobalLatest)
        | Some(UpdateAction::VitePlusGlobalLatest)
        | Some(UpdateAction::PnpmGlobalLatest) => {
            let latest_version = fetch_latest_github_release_version(&client_pool).await?;
            let package_info = client_pool
                .get(npm_registry::PACKAGE_URL)
                .headers(default_headers())
                .send()
                .await?
                .error_for_status()?
                .json::<NpmPackageInfo>()
                .await?;
            npm_registry::ensure_version_ready(&package_info, &latest_version)?;
            (latest_version, None)
        }
        Some(UpdateAction::StandaloneUnix) | Some(UpdateAction::StandaloneWindows) | None => (
            fetch_latest_github_release_version(&client_pool).await?,
            None,
        ),
    };

    // Preserve any previously dismissed version if present.
    let prev_info = read_version_info(version_file).ok();
    let info = VersionInfo {
        latest_version,
        latest_asset_sha256,
        last_checked_at: Utc::now(),
        dismissed_version: prev_info.and_then(|p| p.dismissed_version),
    };

    let json_line = format!("{}\n", serde_json::to_string(&info)?);
    if let Some(parent) = version_file.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(version_file, json_line).await?;
    Ok(())
}

async fn fetch_latest_github_release_version(
    client_pool: &RouteAwareClientPool,
) -> anyhow::Result<String> {
    let ReleaseInfo {
        tag_name: latest_tag_name,
    } = client_pool
        .get(LATEST_RELEASE_URL)
        .headers(default_headers())
        .send()
        .await?
        .error_for_status()?
        .json::<ReleaseInfo>()
        .await?;
    extract_version_from_latest_tag(&latest_tag_name)
}

async fn fetch_latest_mikodex_rolling_digest(
    client_pool: &RouteAwareClientPool,
) -> anyhow::Result<String> {
    let release = client_pool
        .get(MIKODEX_ROLLING_RELEASE_URL)
        .headers(default_headers())
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?
        .error_for_status()?
        .json::<MikodexRollingRelease>()
        .await?;
    anyhow::ensure!(
        release.tag_name == MIKODEX_ROLLING_TAG && !release.draft && release.prerelease,
        "the Mikodex rolling release is missing or is not published as a prerelease"
    );

    let mut assets = release
        .assets
        .iter()
        .filter(|asset| asset.name == MIKODEX_ROLLING_PACKAGE_ASSET);
    let asset = assets
        .next()
        .ok_or_else(|| anyhow::anyhow!("Mikodex rolling release is missing its Windows package"))?;
    anyhow::ensure!(
        assets.next().is_none(),
        "Mikodex rolling release contains duplicate Windows packages"
    );

    let digest = asset
        .digest
        .as_deref()
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| {
            anyhow::anyhow!("GitHub did not provide a SHA-256 digest for the Mikodex package")
        })?;
    anyhow::ensure!(
        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "GitHub provided an invalid Mikodex package SHA-256 digest"
    );
    Ok(digest)
}

fn mikodex_rolling_receipt_matches(asset_digest: &str) -> bool {
    #[derive(Deserialize)]
    struct UpdateReceipt {
        tag_name: String,
        asset_sha256: String,
        package_version: String,
    }

    let context = InstallContext::current();
    let Some(root) = update_action::mikodex_rolling_package_root(context) else {
        return false;
    };
    let Some(package_manifest) = context.package_manifest() else {
        return false;
    };
    let Ok(contents) = std::fs::read(root.join(".mikodex-update.json")) else {
        return false;
    };
    let Ok(receipt) = serde_json::from_slice::<UpdateReceipt>(&contents) else {
        return false;
    };

    receipt.tag_name == MIKODEX_ROLLING_TAG
        && receipt.asset_sha256 == asset_digest
        && receipt.package_version == package_manifest.version.to_string()
}

/// Returns the latest version to show in a popup, if it should be shown.
/// This respects the user's dismissal choice for the current latest version.
pub fn get_upgrade_version_for_popup(config: &Config) -> Option<String> {
    if !config.check_for_update_on_startup || is_source_build_version(CODEX_CLI_VERSION) {
        return None;
    }

    let version_file = version_filepath(config);
    let latest = get_upgrade_version(config)?;
    // If the user dismissed this exact version previously, do not show the popup.
    if let Ok(info) = read_version_info(&version_file)
        && info.dismissed_version.as_deref() == Some(latest.as_str())
    {
        return None;
    }
    Some(latest)
}
