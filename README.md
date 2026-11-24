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
  with short descriptions when the terminal is wide enough (120 columns or
  more).
- **Side conversations beside the main chat.** In the full-screen terminal
  layout, `/side` keeps the main thread visible on the left and opens the side
  thread on the right. Click either pane or use `Ctrl+/` to switch which
  conversation receives input.
- **Keyboard focus across panes.** With an empty chat draft, use the left and
  right arrows to move through the Agent Command Center, main chat, optional
  side chat, and command reference. The right reference scrolls with Up and
  Down when focused. Enter in the Agent Command Center still opens the selected
  session; arrows keep editing a nonempty draft while chat has focus.
- **Resizable sidebars and Agent Command Center details pane.** Drag dock dividers with the mouse, or use the
  keyboard controls below. The Agent Command Center details pane also has a mouse-draggable divider. Mikodex saves each width in your user config.
- **Korean keyboard input for slash commands.** Slash commands work when typed
  with a Korean 2-set keyboard layout active. For example, `/벼ㅑㅅ` and `/ㄷ턋`
  are accepted as `/quit` and `/exit`; the same conversion works for other
  built-in commands.
- **Animated chat backgrounds.** The built-in Codex logo animation runs when no
  personal background image is configured. During chat sessions, the animation
  or selected image remains visible as a subtle watermark in unused transcript
  cells.
- **Rolling release updates.** On Windows x64 package installs,
  `mikodex update` checks the Mikodex rolling release and its SHA-256 digest.
  It detects rebuilt files even when the package version stays the same, verifies
  the complete ZIP before installing, and replaces the package after the update
  command exits.
- **Startup update prompt.** Windows x64 package installs check the rolling
  release during startup and offer Update now, Skip, or Skip until next version.
  The check detects a rebuilt package when its version number is unchanged.

## Build and run

For quick source development on macOS, Linux, or Windows through WSL2, run:

```sh
cargo run --manifest-path codex-rs/Cargo.toml -p codex-cli --bin mikodex
```

That command builds a development executable, not a complete local package. To
build the complete package with its runtime helpers, use the canonical package
builder:

```sh
just assemble-codex-package --variant codex --cargo-profile release --package-dir codex-rs/target/mikodex-package --force
```

Run `codex-rs/target/mikodex-package/bin/mikodex.exe` on Windows or
`codex-rs/target/mikodex-package/bin/mikodex` on macOS and Linux. The package
includes the metadata and helper binaries required by daemon installation and
sandbox setup.

Native Windows x64 builds are also supported. Install the Rust
`x86_64-pc-windows-msvc` toolchain and Visual Studio 2022 Build Tools with the
**Desktop development with C++** workload and a Windows SDK. Run the same Cargo
package build from an x64 Native Tools Command Prompt or Developer PowerShell so
the MSVC environment is configured. The local `.toolchain/build-mikodex.cmd`
script runs this canonical package build. See [Rust installation](https://rust-lang.org/tools/install/),
[MSVC command-line setup](https://learn.microsoft.com/en-us/cpp/build/building-on-the-command-line?view=msvc-170),
and the repository [build instructions](docs/install.md).

To sign in, run `mikodex login` and choose the ChatGPT sign-in option. Mikodex
uses the upstream Codex authentication implementation.

## Updating Mikodex

On Windows x64, run `mikodex update` to check the latest
[Mikodex rolling release](https://github.com/2-sl/mikodex/releases/tag/mikodex-rolling).
The updater compares the published package SHA-256, so it notices when a new
ZIP replaces the rolling release asset even if its version number is unchanged.
It verifies the release checksum and the complete package before installing.
Windows keeps a running executable locked, so a helper finishes the package swap
after the update command exits, without showing a completion dialog.
Start Mikodex again after the update command exits. This updater currently supports the
canonical Windows x64 package layout.

## Resizing the sidebars

Drag a vertical divider to resize the Agent Command Center or command reference
sidebar. For keyboard resizing, press `Ctrl+X`, then `R` (the default
`resize_panes` shortcut). Use the left and right arrow keys to adjust by one
column, or `Shift+Left` and `Shift+Right` to adjust by ten columns. Press
`Enter` to save the preview or `Esc` to cancel it. The widths are stored as
`tui.agents_overview_width_percent` and `tui.command_reference_width_percent`
in your Mikodex config. The `/side` conversation split uses
`tui.sidebar_width_percent` and defaults to equal widths.

When the full-screen transcript layout is active, `/side` keeps the main
conversation visible while the side session runs in a second pane. Click either
pane or press `Ctrl+/` (the default side-conversation toggle) to switch focus.
Drag the divider or use the keyboard resize shortcut above to change the split;
the side pane starts at half the central chat area's width.

When the chat draft is empty, the left and right arrows move keyboard focus
between visible panes. The main chat moves left to the Agent Command Center;
moving right enters an open side conversation, then the command reference.
From a side conversation, Left returns to the main chat. Focus the command
reference to scroll its list with Up and Down, or click a dock to focus it.
Arrow keys edit a nonempty draft normally while chat has focus.

## Rendering and sidebars

Use these commands during a session; each change is saved in your user config and takes effect immediately.

- /render table on|off enables or disables rich Markdown table rendering. /render or /render table shows the current setting.
- /sidebar agent-command-center on|off (alias: acs) controls the left Agent Command Center rail.
- /sidebar command-list on|off (alias: cl) controls the right command reference rail. /sidebar shows both settings.

The two rails can be toggled independently. The command list still requires a wide terminal.

## Automated checks

Pushes and pull requests do not start CI checks automatically in this fork.
Run the relevant build and tests locally. Some test workflows can still be
started manually from GitHub Actions; release and issue workflows have their
own triggers.

## Local data

By default Mikodex stores its data under `%USERPROFILE%\.mikodex` on Windows
and `~/.mikodex` on macOS and Linux. Set `MIKODEX_HOME` to use another directory.

## Animated chat background

Set a personal image with `/background "<image path>"` or its `/bga` alias.
Mikodex copies it to
`$MIKODEX_HOME/background/custom.png` (normally `%USERPROFILE%\.mikodex\background\custom.png`)
and reuses that copy on later launches. PNG, JPEG, GIF, and WebP input files are
supported. Use `/background off` to disable the animation, `/background on` to
enable it again, and `/background clear` to remove the saved image while keeping
the current enabled or disabled state. The off/on setting persists across launches.

This repository is licensed under the [Apache-2.0 License](LICENSE). See
[contributing](docs/contributing.md) and [build instructions](docs/install.md)
for repository development details.
