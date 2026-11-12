# Mikodex

Mikodex is an unofficial, independently maintained project derived from the
OpenAI Codex CLI. It is not affiliated with, endorsed by, or sponsored by OpenAI.
Its command is `mikodex`, and its configuration, login tokens, session history,
and background-server state use a separate home directory so they do not share
state with an installed Codex.

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

Set a personal image as the animated chat background with
`/background "<image path>"`. Mikodex copies it to
`$MIKODEX_HOME/background/custom.png` (normally `%USERPROFILE%\.mikodex\background\custom.png`)
and reuses that copy on later launches. PNG, JPEG, GIF, and WebP input files are
supported. Use `/background off` to remove the saved image.

This repository is licensed under the [Apache-2.0 License](LICENSE). See
[contributing](docs/contributing.md) and [build instructions](docs/install.md)
for repository development details.
