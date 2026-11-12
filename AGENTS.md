# Mikodex repository instructions

## Git and remote safety

- Never push, publish, or open a pull request against `https://github.com/openai/codex.git`.
- Treat the upstream Codex repository as read-only; fetching releases is allowed.
- After committing changes on the active Mikodex branch, immediately push that branch to `origin` when it points to `https://github.com/2-sl/mikodex.git`, unless the user explicitly says to hold the push. Do not force-push.
- Do not push to any other remote unless the user explicitly asks for that push.
- Make descriptive local Git commits at meaningful milestones.

## Stable-release updates

- When asked to update Mikodex to the latest Codex release, use the newest official stable `rust-v*` source release tag from upstream; exclude prerelease tags.
- Keep Mikodex changes on top of the selected stable release and rebuild the Windows binary.
- Review the resulting changes before committing locally. Never push to the upstream Codex repository.

## Complete local builds

- For a usable local CLI package, use the canonical builder at `scripts/build_codex_package.py` (or `just assemble-codex-package`). A raw `cargo build --bin mikodex` only produces the entrypoint and is not a complete package.
- The canonical package must include its metadata, code-mode host, exec server, ripgrep, and platform helpers. Windows packages specifically require `codex-command-runner.exe` and `codex-windows-sandbox-setup.exe` under `codex-resources`.
- `.toolchain/build-mikodex.cmd` must build the canonical Windows package at `codex-rs/target/mikodex-package`. Point the user's `mikodex` command to `codex-rs/target/mikodex-package/bin`, never to the raw `codex-rs/target/release` executable.
