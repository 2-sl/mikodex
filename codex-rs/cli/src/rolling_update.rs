use anyhow::Context;
use anyhow::bail;
use anyhow::ensure;
use codex_http_client::ClientRouteClass;
use codex_http_client::HttpClientFactory;
use codex_http_client::OutboundProxyPolicy;
use codex_http_client::RouteAwareClientPool;
use codex_login::default_client::default_headers;
use futures::StreamExt;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;
use std::collections::BTreeMap;
use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use zip::ZipArchive;

const RELEASE_API_URL: &str =
    "https://api.github.com/repos/2-sl/mikodex/releases/tags/mikodex-rolling";
const ROLLING_TAG: &str = "mikodex-rolling";
const PACKAGE_ASSET: &str = "mikodex-windows-x64.zip";
const CHECKSUM_ASSET: &str = "mikodex-windows-x64.zip.sha256";
const RECEIPT_FILE: &str = ".mikodex-update.json";
const MAX_ARCHIVE_SIZE: u64 = 2 * 1024 * 1024 * 1024;
const MAX_UNPACKED_SIZE: u64 = 2 * 1024 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 20_000;

#[derive(Debug, Deserialize)]
struct ReleaseInfo {
    tag_name: String,
    name: Option<String>,
    draft: bool,
    prerelease: bool,
    assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
    size: u64,
    digest: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PackageMetadata {
    #[serde(rename = "layoutVersion")]
    layout_version: u8,
    version: String,
    target: String,
    variant: String,
    entrypoint: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct UpdateReceipt {
    tag_name: String,
    asset_sha256: String,
    package_version: String,
}

struct Install {
    root: PathBuf,
    version: String,
}

pub async fn run_if_supported() -> anyhow::Result<bool> {
    let Some(install) = current_install() else {
        return Ok(false);
    };

    update(install).await?;
    Ok(true)
}

fn current_install() -> Option<Install> {
    let layout = codex_install_context::InstallContext::current()
        .package_layout
        .as_ref()?;
    let root = layout.package_dir.as_path().to_path_buf();
    let metadata = read_package_metadata(&root).ok()?;
    if metadata.layout_version != 1
        || metadata.target != "x86_64-pc-windows-msvc"
        || metadata.variant != "codex"
        || metadata.entrypoint != "bin/mikodex.exe"
    {
        return None;
    }

    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    let package_exe = root.join("bin/mikodex.exe").canonicalize().ok()?;
    if exe != package_exe {
        return None;
    }

    Some(Install {
        root,
        version: metadata.version,
    })
}

async fn update(install: Install) -> anyhow::Result<()> {
    let client = RouteAwareClientPool::new(
        HttpClientFactory::new(OutboundProxyPolicy::RespectSystemProxy),
        ClientRouteClass::Other,
    )
    .with_legacy_custom_ca_fallback();

    println!("Checking the Mikodex rolling release...");
    let release = client
        .get(RELEASE_API_URL)
        .headers(default_headers())
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .context("failed to contact the Mikodex rolling release")?
        .error_for_status()
        .context("GitHub returned an error for the Mikodex rolling release")?
        .json::<ReleaseInfo>()
        .await
        .context("GitHub returned invalid rolling release metadata")?;
    ensure!(
        release.tag_name == ROLLING_TAG && !release.draft && release.prerelease,
        "the Mikodex rolling release is missing or is not published as a prerelease"
    );

    let archive_asset = unique_asset(&release, PACKAGE_ASSET)?;
    let checksum_asset = unique_asset(&release, CHECKSUM_ASSET)?;
    ensure!(
        archive_asset.size > 0 && archive_asset.size <= MAX_ARCHIVE_SIZE,
        "the rolling package archive has an unexpected size"
    );
    let archive_digest = asset_digest(archive_asset)?;
    if receipt_matches(&install.root, &archive_digest, &install.version) {
        println!(
            "Mikodex {} is up to date ({})",
            install.version,
            release.name.as_deref().unwrap_or(ROLLING_TAG)
        );
        return Ok(());
    }

    println!(
        "Downloading {} to compare with the installed package...",
        release.name.as_deref().unwrap_or(ROLLING_TAG)
    );
    let install_parent = install
        .root
        .parent()
        .context("cannot locate the Mikodex package directory")?;
    let work = tempfile::Builder::new()
        .prefix(".mikodex-update-")
        .tempdir_in(install_parent)
        .context("failed to create a staging directory beside the Mikodex package")?;
    let archive_path = work.path().join(PACKAGE_ASSET);
    let package_stage = work.path().join("package");
    fs::create_dir(&package_stage).context("failed to create the package staging directory")?;

    let expected_checksum = download_checksum(&client, checksum_asset).await?;
    ensure!(
        expected_checksum == archive_digest,
        "the GitHub release checksum file does not match the archive digest in its API metadata"
    );
    download_archive(&client, archive_asset, &archive_path, &archive_digest).await?;
    let package_version = extract_and_validate(&archive_path, &package_stage)?;

    if package_trees_match(&install.root, &package_stage)? {
        write_receipt(
            &install.root,
            &release.tag_name,
            &archive_digest,
            &package_version,
        )?;
        println!(
            "The rolling release asset changed, but the installed package files already match. Mikodex {} is up to date.",
            package_version
        );
        return Ok(());
    }

    write_receipt(
        &package_stage,
        &release.tag_name,
        &archive_digest,
        &package_version,
    )?;
    let log_path = update_log_path()?;
    let work_path = work.keep();
    let staged_package = work_path.join("package");
    if let Err(error) = launch_windows_swap_helper(
        &install.root,
        &staged_package,
        &work_path,
        &log_path,
        &package_version,
    ) {
        let _ = fs::remove_dir_all(&work_path);
        return Err(error);
    }

    println!(
        "Downloaded and verified Mikodex {}. Windows will replace the package after this command exits and show a confirmation dialog. Update log: {}",
        package_version,
        log_path.display()
    );
    Ok(())
}

fn unique_asset<'a>(release: &'a ReleaseInfo, name: &str) -> anyhow::Result<&'a ReleaseAsset> {
    let mut matching = release.assets.iter().filter(|asset| asset.name == name);
    let asset = matching
        .next()
        .with_context(|| format!("rolling release is missing the {name} asset"))?;
    ensure!(
        matching.next().is_none(),
        "rolling release contains duplicate {name} assets"
    );
    Ok(asset)
}

fn asset_digest(asset: &ReleaseAsset) -> anyhow::Result<String> {
    let digest = asset
        .digest
        .as_deref()
        .with_context(|| format!("GitHub did not provide a SHA-256 digest for {}", asset.name))?;
    let digest = digest
        .strip_prefix("sha256:")
        .context("GitHub provided an unsupported digest format")?
        .to_ascii_lowercase();
    ensure!(
        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "GitHub provided an invalid SHA-256 digest"
    );
    Ok(digest)
}

async fn download_checksum(
    client: &RouteAwareClientPool,
    asset: &ReleaseAsset,
) -> anyhow::Result<String> {
    ensure!(
        asset.size > 0 && asset.size <= 4096,
        "checksum asset is too large"
    );
    let expected_asset_digest = asset_digest(asset)?;
    let bytes = client
        .get(&asset.browser_download_url)
        .headers(default_headers())
        .send()
        .await
        .context("failed to download the rolling release checksum")?
        .error_for_status()
        .context("GitHub returned an error for the rolling release checksum")?
        .bytes()
        .await
        .context("failed to read the rolling release checksum")?;
    ensure!(
        bytes.len() as u64 == asset.size,
        "the downloaded checksum size does not match GitHub release metadata"
    );
    ensure!(
        sha256(&bytes) == expected_asset_digest,
        "the downloaded checksum failed GitHub's SHA-256 verification"
    );
    let text = std::str::from_utf8(&bytes).context("release checksum is not valid UTF-8")?;
    let checksum = text
        .split_whitespace()
        .next()
        .context("release checksum file is empty")?
        .to_ascii_lowercase();
    ensure!(
        checksum.len() == 64 && checksum.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "release checksum file does not contain a SHA-256 digest"
    );
    Ok(checksum)
}

async fn download_archive(
    client: &RouteAwareClientPool,
    asset: &ReleaseAsset,
    destination: &Path,
    expected_digest: &str,
) -> anyhow::Result<()> {
    let response = client
        .get(&asset.browser_download_url)
        .headers(default_headers())
        .send()
        .await
        .context("failed to download the Mikodex package")?
        .error_for_status()
        .context("GitHub returned an error for the Mikodex package")?;
    if let Some(content_length) = response.content_length() {
        ensure!(
            content_length == asset.size,
            "downloaded package size does not match GitHub release metadata"
        );
    }

    let mut output = fs::File::create(destination).context("failed to create package download")?;
    let mut stream = response.bytes_stream();
    let mut hasher = Sha256::new();
    let mut downloaded = 0_u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("failed while downloading the Mikodex package")?;
        downloaded = downloaded
            .checked_add(chunk.len() as u64)
            .context("package download size overflowed")?;
        ensure!(
            downloaded <= asset.size,
            "package download exceeded its advertised size"
        );
        output
            .write_all(&chunk)
            .context("failed to write the package download")?;
        hasher.update(&chunk);
    }
    output
        .flush()
        .context("failed to flush the package download")?;
    ensure!(
        downloaded == asset.size,
        "downloaded package size does not match GitHub release metadata"
    );
    let actual_digest = format!("{:x}", hasher.finalize());
    ensure!(
        actual_digest == expected_digest,
        "downloaded package failed GitHub's SHA-256 verification"
    );
    Ok(())
}

fn extract_and_validate(archive_path: &Path, package_stage: &Path) -> anyhow::Result<String> {
    let archive_file = fs::File::open(archive_path).context("failed to open package archive")?;
    let mut archive =
        ZipArchive::new(archive_file).context("rolling package is not a valid ZIP")?;
    ensure!(
        archive.len() > 0 && archive.len() <= MAX_ARCHIVE_ENTRIES,
        "rolling package contains an unexpected number of files"
    );
    let mut paths = HashSet::with_capacity(archive.len());
    let mut total_size = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .with_context(|| format!("failed to read package archive entry {index}"))?;
        let relative_path = entry
            .enclosed_name()
            .context("package archive contains an unsafe file path")?
            .to_path_buf();
        ensure!(
            !relative_path.as_os_str().is_empty() && paths.insert(relative_path.clone()),
            "package archive contains a duplicate or empty path"
        );
        total_size = total_size
            .checked_add(entry.size())
            .context("package expanded size overflowed")?;
        ensure!(
            total_size <= MAX_UNPACKED_SIZE,
            "package expanded size exceeds the safety limit"
        );
        let destination = package_stage.join(relative_path);
        if entry.is_dir() {
            fs::create_dir_all(&destination).context("failed to create a package directory")?;
            continue;
        }
        let parent = destination
            .parent()
            .context("package archive contains an invalid file path")?;
        fs::create_dir_all(parent).context("failed to create a package directory")?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)
            .context("failed to create an extracted package file")?;
        let copied =
            std::io::copy(&mut entry, &mut output).context("failed to extract a package file")?;
        ensure!(
            copied == entry.size(),
            "extracted package file has an unexpected size"
        );
    }

    let metadata = read_package_metadata(package_stage)?;
    ensure!(
        metadata.layout_version == 1
            && metadata.target == "x86_64-pc-windows-msvc"
            && metadata.variant == "codex"
            && metadata.entrypoint == "bin/mikodex.exe",
        "rolling release does not contain the expected Windows x64 Mikodex package"
    );
    for required in [
        "bin/mikodex.exe",
        "bin/codex-code-mode-host.exe",
        "bin/codex-exec-server.exe",
        "codex-path/rg.exe",
        "codex-resources/codex-command-runner.exe",
        "codex-resources/codex-windows-sandbox-setup.exe",
    ] {
        ensure!(
            package_stage.join(required).is_file(),
            "rolling package is incomplete: missing {required}"
        );
    }
    Ok(metadata.version)
}

fn read_package_metadata(root: &Path) -> anyhow::Result<PackageMetadata> {
    let path = root.join("codex-package.json");
    let contents = fs::read(&path)
        .with_context(|| format!("failed to read package metadata at {}", path.display()))?;
    serde_json::from_slice(&contents).context("package metadata is invalid JSON")
}

fn package_trees_match(installed: &Path, staged: &Path) -> anyhow::Result<bool> {
    let installed_files = package_file_digests(installed)?;
    let staged_files = package_file_digests(staged)?;
    Ok(installed_files == staged_files)
}

fn package_file_digests(root: &Path) -> anyhow::Result<BTreeMap<PathBuf, String>> {
    let mut files = BTreeMap::new();
    collect_package_file_digests(root, root, &mut files)?;
    Ok(files)
}

fn collect_package_file_digests(
    root: &Path,
    directory: &Path,
    files: &mut BTreeMap<PathBuf, String>,
) -> anyhow::Result<()> {
    for entry in fs::read_dir(directory).context("failed to read package directory")? {
        let entry = entry.context("failed to read a package directory entry")?;
        let path = entry.path();
        let relative_path = path
            .strip_prefix(root)
            .context("package path escaped its root directory")?;
        if relative_path == Path::new(RECEIPT_FILE) {
            continue;
        }
        let file_type = entry
            .file_type()
            .context("failed to inspect a package file")?;
        if file_type.is_symlink() {
            bail!(
                "package contains a symbolic link: {}",
                relative_path.display()
            );
        }
        if file_type.is_dir() {
            collect_package_file_digests(root, &path, files)?;
        } else if file_type.is_file() {
            files.insert(relative_path.to_path_buf(), file_sha256(&path)?);
        } else {
            bail!(
                "package contains an unsupported file: {}",
                relative_path.display()
            );
        }
    }
    Ok(())
}

fn file_sha256(path: &Path) -> anyhow::Result<String> {
    let mut file = fs::File::open(path).context("failed to open a package file")?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .context("failed to read a package file for comparison")?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn receipt_matches(root: &Path, asset_digest: &str, installed_version: &str) -> bool {
    let Ok(contents) = fs::read(root.join(RECEIPT_FILE)) else {
        return false;
    };
    let Ok(receipt) = serde_json::from_slice::<UpdateReceipt>(&contents) else {
        return false;
    };
    receipt.tag_name == ROLLING_TAG
        && receipt.asset_sha256 == asset_digest
        && receipt.package_version == installed_version
}

fn write_receipt(
    root: &Path,
    tag_name: &str,
    asset_sha256: &str,
    package_version: &str,
) -> anyhow::Result<()> {
    let receipt = UpdateReceipt {
        tag_name: tag_name.to_string(),
        asset_sha256: asset_sha256.to_string(),
        package_version: package_version.to_string(),
    };
    let mut contents = serde_json::to_vec_pretty(&receipt)?;
    contents.push(b'\n');
    fs::write(root.join(RECEIPT_FILE), contents)
        .context("failed to record the installed rolling release digest")
}

fn update_log_path() -> anyhow::Result<PathBuf> {
    let local_app_data = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .context("LOCALAPPDATA is not set")?;
    let log_dir = local_app_data.join("Mikodex");
    fs::create_dir_all(&log_dir).context("failed to create the Mikodex update log directory")?;
    Ok(log_dir.join("update.log"))
}

fn launch_windows_swap_helper(
    install_root: &Path,
    staged_package: &Path,
    work_dir: &Path,
    log_path: &Path,
    package_version: &str,
) -> anyhow::Result<()> {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let system_root = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .context("SystemRoot is not set")?;
    let powershell = system_root
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    ensure!(powershell.is_file(), "Windows PowerShell was not found");

    let script_path = std::env::temp_dir().join(format!(
        "mikodex-update-{}-{}.ps1",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .context("system clock is before Unix epoch")?
            .as_nanos()
    ));
    fs::write(&script_path, WINDOWS_SWAP_SCRIPT).context("failed to write update helper")?;

    let result = std::process::Command::new(&powershell)
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-File")
        .arg(&script_path)
        .arg("-ParentPid")
        .arg(std::process::id().to_string())
        .arg("-InstallRoot")
        .arg(install_root)
        .arg("-StagedPackage")
        .arg(staged_package)
        .arg("-WorkDir")
        .arg(work_dir)
        .arg("-LogPath")
        .arg(log_path)
        .arg("-PackageVersion")
        .arg(package_version)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();

    if result.is_err() {
        let _ = fs::remove_file(&script_path);
        result.context("failed to start the Windows package updater")?;
    }
    Ok(())
}

const WINDOWS_SWAP_SCRIPT: &str = r#"
param(
    [Parameter(Mandatory=$true)][int]$ParentPid,
    [Parameter(Mandatory=$true)][string]$InstallRoot,
    [Parameter(Mandatory=$true)][string]$StagedPackage,
    [Parameter(Mandatory=$true)][string]$WorkDir,
    [Parameter(Mandatory=$true)][string]$LogPath,
    [Parameter(Mandatory=$true)][string]$PackageVersion
)
$ErrorActionPreference = 'Stop'
$backup = "$InstallRoot.mikodex-backup-$ParentPid"
$movedOldPackage = $false
try {
    for ($attempt = 0; $attempt -lt 120; $attempt++) {
        if (-not (Get-Process -Id $ParentPid -ErrorAction SilentlyContinue)) { break }
        Start-Sleep -Seconds 1
    }
    if (Get-Process -Id $ParentPid -ErrorAction SilentlyContinue) {
        throw 'The Mikodex process did not exit in time.'
    }
    if (Test-Path -LiteralPath $backup) {
        throw "An update backup already exists: $backup"
    }
    Move-Item -LiteralPath $InstallRoot -Destination $backup
    $movedOldPackage = $true
    try {
        Move-Item -LiteralPath $StagedPackage -Destination $InstallRoot
    } catch {
        Move-Item -LiteralPath $backup -Destination $InstallRoot
        $movedOldPackage = $false
        throw
    }
    try {
        Remove-Item -LiteralPath $backup -Recurse -Force
    } catch {
        Add-Content -LiteralPath $LogPath -Value "Installed Mikodex $PackageVersion; old package cleanup failed: $_"
    }
    Add-Content -LiteralPath $LogPath -Value "Installed Mikodex $PackageVersion successfully."
    try {
        Add-Type -AssemblyName System.Windows.Forms
        [System.Windows.Forms.MessageBox]::Show("Mikodex $PackageVersion is installed. Start Mikodex again to use it.", 'Mikodex update complete', 'OK', 'Information') | Out-Null
    } catch {}
    Remove-Item -LiteralPath $WorkDir -Recurse -Force -ErrorAction SilentlyContinue
} catch {
    if ($movedOldPackage -and -not (Test-Path -LiteralPath $InstallRoot) -and (Test-Path -LiteralPath $backup)) {
        Move-Item -LiteralPath $backup -Destination $InstallRoot -ErrorAction SilentlyContinue
    }
    Add-Content -LiteralPath $LogPath -Value "Mikodex update failed: $_"
    try {
        Add-Type -AssemblyName System.Windows.Forms
        [System.Windows.Forms.MessageBox]::Show("Mikodex could not finish updating. The previous package was restored when possible. See $LogPath", 'Mikodex update failed', 'OK', 'Error') | Out-Null
    } catch {}
    Remove-Item -LiteralPath $WorkDir -Recurse -Force -ErrorAction SilentlyContinue
} finally {
    Remove-Item -LiteralPath $PSCommandPath -Force -ErrorAction SilentlyContinue
}
"#;
