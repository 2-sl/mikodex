//! Bind filesystem and network inputs for sandbox preparation on the execution host.

use crate::SandboxExecRequest;
use crate::SandboxIntegrityRequest;
use crate::SandboxType;
use crate::SpawnOptions;
use crate::SpawnRequest;
use crate::WindowsSandboxFilesystemOverrides;
use crate::WindowsSandboxProxySettingsMode;
use crate::WindowsSandboxSpawnRequest;
use crate::spawn::spawn_process;
use anyhow::Context;
use codex_protocol::config_types::WindowsSandboxLevel;
use codex_protocol::sandbox::SandboxOverride;
use codex_utils_pty::SpawnedProcess;
use std::collections::HashMap;
use std::io;
use std::process::Command;

use codex_protocol::models::PermissionProfile;
use codex_protocol::permissions::FileSystemPath;
use codex_utils_absolute_path::AbsolutePathBuf;
use codex_utils_path_uri::PathUri;

pub(super) struct WindowsSessionPolicy {
    pub(super) workspace_roots: Vec<AbsolutePathBuf>,
    pub(super) filesystem_overrides: Option<WindowsSandboxFilesystemOverrides>,
    pub(super) proxy_enforced: bool,
    #[cfg(windows)]
    pub(super) restricting_sid: Option<String>,
    pub(super) proxy_settings_mode: WindowsSandboxProxySettingsMode,
}

/// A command prepared for its selected sandbox. Native Windows token sessions
/// must use the Windows session launcher.
#[must_use]
pub struct WrappedSandboxCommand {
    pub(super) request: SandboxExecRequest,
    pub(super) inner_arg_count: usize,
    pub(super) windows_session: Option<Box<WindowsSessionPolicy>>,
}

impl WrappedSandboxCommand {
    /// Prepare a native command with the executor's validated working directory and child environment.
    pub fn native(
        argv: Vec<String>,
        cwd: PathUri,
        env: HashMap<String, String>,
        arg0: Option<String>,
    ) -> io::Result<Self> {
        if argv.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "argv must not be empty",
            ));
        }
        Ok(Self {
            inner_arg_count: argv.len(),
            request: SandboxExecRequest {
                sandbox_override: SandboxOverride::NoOverride,
                command: argv,
                native_program: None,
                sandbox_policy_cwd: cwd.clone(),
                cwd,
                env,
                network: None,
                network_environment_id: None,
                sandbox: SandboxType::None,
                windows_sandbox_level: WindowsSandboxLevel::Disabled,
                permission_profile: PermissionProfile::Disabled,
                arg0,
            },
            windows_session: None,
        })
    }

    /// The backend that will enforce this launch.
    pub fn backend(&self) -> SandboxType {
        self.request.sandbox
    }

    /// Inspect launch dependencies with the controller's approval observation.
    pub fn integrity_request(
        &self,
        sandbox_override: SandboxOverride,
    ) -> SandboxIntegrityRequest<'_> {
        let mut facts = SandboxIntegrityRequest::from(&self.request);
        facts.sandbox_override = sandbox_override;
        facts
    }

    /// Check whether a snapshot can capture this shell invocation inside the prepared launcher.
    pub fn supports_shell_snapshot(&self, expected: &[String]) -> bool {
        self.shell_script_index(expected).is_some()
    }

    fn shell_script_index(&self, expected: &[String]) -> Option<usize> {
        match self.request.sandbox {
            SandboxType::None | SandboxType::MacosSeatbelt | SandboxType::LinuxSeccomp => {}
            // Windows either needs a native session or encodes its workload in the environment.
            SandboxType::WindowsRestrictedToken | SandboxType::WindowsMxc => return None,
        }
        // An argv[0] helper may precede the shell inside the sandbox. Only the submitted
        // invocation can match, never a suffix reaching into the enclosing sandbox launcher.
        if expected.len() < 3 || expected.len() > self.inner_arg_count {
            return None;
        }
        let prefix_len = self.request.command.strip_suffix(expected)?.len();
        self.request.cwd.to_abs_path().ok()?;
        Some(prefix_len + 2)
    }

    /// Build a capture command by replacing only the matched shell script.
    pub fn shell_snapshot_capture_command(
        &self,
        expected: &[String],
        script: &str,
    ) -> Option<Command> {
        let index = self.shell_script_index(expected)?;
        let request = &self.request;
        let (program, _) = request.program_and_args()?;
        let mut command = Command::new(program);
        command
            .args(&request.command[1..index])
            .arg(script)
            .args(&request.command[index + 1..])
            .current_dir(request.cwd.to_abs_path().ok()?)
            .env_clear()
            .envs(&request.env);
        #[cfg(unix)]
        if let Some(arg0) = &request.arg0 {
            use std::os::unix::process::CommandExt;
            command.arg0(arg0);
        }
        Some(command)
    }

    /// Borrow the matched shell's flag, script, and environment for snapshot replay.
    pub fn shell_snapshot_replay_mut(
        &mut self,
        expected: &[String],
    ) -> Option<(&mut String, &mut String, &mut HashMap<String, String>)> {
        let index = self.shell_script_index(expected)?;
        let [flag, script] = self.request.command[index - 1..].first_chunk_mut()?;
        Some((flag, script, &mut self.request.env))
    }

    /// Spawn the command in its prepared sandbox.
    pub async fn spawn(self, options: SpawnOptions<'_>) -> anyhow::Result<SpawnedProcess> {
        let request = self.request;
        let windows_sandbox =
            self.windows_session
                .as_deref()
                .map(|session| WindowsSandboxSpawnRequest {
                    permission_profile: &request.permission_profile,
                    workspace_roots: &session.workspace_roots,
                    windows_sandbox_level: request.windows_sandbox_level,
                    proxy_enforced: session.proxy_enforced,
                    #[cfg(windows)]
                    network_proxy_restricting_sid: session.restricting_sid.as_deref(),
                    #[cfg(not(windows))]
                    network_proxy_restricting_sid: None,
                    proxy_settings_mode: session.proxy_settings_mode,
                    filesystem_overrides: session.filesystem_overrides.as_ref(),
                });
        let cwd = request.cwd.to_abs_path().context("invalid process cwd")?;
        let (program, args) = request
            .program_and_args()
            .context("missing program for process spawn")?;
        spawn_process(SpawnRequest {
            program,
            args,
            cwd: cwd.as_path(),
            env: &request.env,
            arg0: &request.arg0,
            sandbox: request.sandbox,
            windows_sandbox,
            tty: options.tty,
            stdin_open: options.stdin_open,
            inherited_fds: options.inherited_fds,
        })
        .await
    }

    /// Return the request for a caller that supplies its own process launcher.
    pub fn into_command_request(self) -> io::Result<SandboxExecRequest> {
        if self.windows_session.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "a native Windows sandbox requires its session launcher",
            ));
        }
        Ok(self.request)
    }

    /// Return a configured process command when argv carries all required sandbox enforcement.
    pub fn into_direct_command(self) -> io::Result<Command> {
        let request = self.request;
        if request.sandbox == SandboxType::WindowsRestrictedToken {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "a native Windows sandbox requires its session launcher",
            ));
        }
        let (program, args) = request.program_and_args().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "sandbox command is empty")
        })?;
        let mut command = Command::new(program);
        #[cfg(unix)]
        if let Some(arg0) = &request.arg0 {
            use std::os::unix::process::CommandExt;
            command.arg0(arg0);
        }
        command
            .args(args)
            .current_dir(request.cwd.to_abs_path()?)
            .env_clear()
            .envs(request.env);
        Ok(command)
    }
}

/// The authorized profile, resolved on the host that will enforce it.
#[derive(Debug)]
pub(super) struct PreparedLaunchPolicy {
    pub(super) permissions: PermissionProfile,
}

impl PreparedLaunchPolicy {
    /// An empty workspace list stays empty, even when the policy contains `:workspace_roots`.
    pub(super) fn resolve(
        permissions: PermissionProfile,
        policy_cwd: &PathUri,
        roots: Option<&[PathUri]>,
        runtime_readable_paths: &[AbsolutePathBuf],
    ) -> io::Result<Self> {
        let Some(roots) = roots else {
            if !runtime_readable_paths.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "runtime read access requires explicit workspace roots",
                ));
            }
            return Ok(Self { permissions });
        };
        let cwd = native_path(policy_cwd, "sandbox policy cwd")?;
        let workspace_roots = native_workspace_roots(roots)?;
        let (file_system, network) = permissions.to_runtime_permissions();
        for entry in &file_system.entries {
            if let FileSystemPath::Path { path } = &entry.path {
                native_path(path, "sandbox permission path")?;
            }
        }
        let file_system = file_system
            .materialize_project_roots_with_workspace_roots(&workspace_roots)
            .with_additional_readable_roots(&cwd, runtime_readable_paths);
        let permissions = PermissionProfile::from_runtime_permissions_with_enforcement(
            permissions.enforcement(),
            &file_system,
            network,
        );
        Ok(Self { permissions })
    }
}

pub(super) fn native_workspace_roots(roots: &[PathUri]) -> io::Result<Vec<AbsolutePathBuf>> {
    roots
        .iter()
        .map(|root| native_path(root, "sandbox workspace root"))
        .collect::<io::Result<Vec<_>>>()
}

fn native_path(path: &PathUri, field: &str) -> io::Result<AbsolutePathBuf> {
    path.to_abs_path()
        .map_err(|err| io::Error::new(err.kind(), format!("invalid {field}: {err}")))
}
