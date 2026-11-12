## Installing & building

### System requirements

| Requirement                 | Details                                                         |
| --------------------------- | --------------------------------------------------------------- |
| Operating systems           | macOS 12+, Ubuntu 20.04+/Debian 10+, or Windows 11 (native x64/MSVC or WSL2) |
| Git (optional, recommended) | 2.23+ for built-in PR helpers                                   |
| RAM                         | 4-GB minimum (8-GB recommended)                                 |

### Build from source

Native Windows x64 builds use the `x86_64-pc-windows-msvc` Rust toolchain and
Visual Studio 2022 Build Tools with the **Desktop development with C++**
workload and a Windows SDK. Run Cargo commands from an x64 Native Tools Command
Prompt or Developer PowerShell so the MSVC environment is configured. Install
Rust with the [Windows rustup installer](https://rust-lang.org/tools/install/)
and select the MSVC toolchain. See Microsoft's guide to [MSVC command-line
builds](https://learn.microsoft.com/en-us/cpp/build/building-on-the-command-line?view=msvc-170).

On Windows, install Rust with rustup-init.exe and run the Rust and Cargo
commands from the block below in the Developer shell; skip the `curl` and
`source` lines. The shell installer lines are for macOS, Linux, and WSL2.

```bash
# Run these commands from the repository root.

# Install the Rust toolchain, if necessary.
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
rustup component add rustfmt
rustup component add clippy
# Install helper tools used by the workspace justfile:
cargo install --locked just
# DotSlash fetches pinned development tools such as buildifier on first use.
cargo install --locked dotslash
# Install nextest for the `just test` helper.
cargo install --locked cargo-nextest

# Build Mikodex.
cargo build --manifest-path codex-rs/Cargo.toml -p codex-cli --bin mikodex

# Launch the TUI with a sample prompt.
cargo run --manifest-path codex-rs/Cargo.toml -p codex-cli --bin mikodex -- "explain this codebase to me"

# After making changes, use the root justfile helpers (they default to codex-rs):
just fmt
just fix -p <crate-you-touched>

# Run the relevant tests (project-specific is fastest), for example:
just test -p codex-tui
# `just test` runs the test suite via nextest:
just test
# Avoid `--all-features` for routine local runs because it increases build
# time and `target/` disk usage by compiling additional feature combinations.
```

## Tracing / verbose logging

Mikodex is written in Rust, so it honors the `RUST_LOG` environment variable to configure its logging behavior.

The TUI records diagnostics in bounded local stores by default. Set `log_dir` explicitly to enable a plaintext TUI log for a run:

```bash
mikodex -c log_dir=./.mikodex-log
tail -F ./.mikodex-log/codex-tui.log
```

The non-interactive mode (`mikodex exec`) defaults to `RUST_LOG=error`, but messages are printed inline, so there is no need to monitor a separate file.

See the Rust documentation on [`RUST_LOG`](https://docs.rs/env_logger/latest/env_logger/#enabling-logging) for more information on the configuration options.
