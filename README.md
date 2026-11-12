# Mikodex

Mikodex is an unofficial, independently maintained project derived from the
OpenAI Codex CLI. It is not affiliated with, endorsed by, or sponsored by OpenAI.
Its command is `mikodex`, and its configuration, login tokens, session history,
and background-server state use a separate home directory so they do not share
state with an installed Codex.

## Features

- **Mikodex user-facing name.** The CLI is presented as Mikodex and runs with
  the `mikodex` command. OpenAI-facing APIs, protocols, authentication, and
  service identifiers retain Codex naming where compatibility requires it.
- **Agent Command Center sidebar.** The Agent Command Center stays pinned in a
  left sidebar while the selected agent session appears in the main chat area.
- **Command reference sidebar.** A right sidebar lists available slash commands
  with short descriptions.
- **Animated chat backgrounds.** The built-in Codex logo animation runs when no
  personal background image is configured. During chat sessions, the animation
  or selected image remains visible as a subtle watermark in unused transcript
  cells.

## Build and run

On macOS, Linux, or Windows through WSL2, build and start the CLI from this
repository with:

```sh
cargo run --manifest-path codex-rs/Cargo.toml -p codex-cli --bin mikodex
```

To sign in, run `mikodex login` and choose the ChatGPT sign-in option. Mikodex
uses the upstream Codex authentication implementation.

## Local data

By default Mikodex stores its data under `%USERPROFILE%\.mikodex` on Windows
and `~/.mikodex` on macOS and Linux. Set `MIKODEX_HOME` to use another directory.

## Animated chat background

Set a personal image with `/background "<image path>"`. Mikodex copies it to
`$MIKODEX_HOME/background/custom.png` (normally `%USERPROFILE%\.mikodex\background\custom.png`)
and reuses that copy on later launches. PNG, JPEG, GIF, and WebP input files are
supported. Use `/background off` to remove the saved image.

This repository is licensed under the [Apache-2.0 License](LICENSE). See
[contributing](docs/contributing.md) and [build instructions](docs/install.md)
for repository development details.
