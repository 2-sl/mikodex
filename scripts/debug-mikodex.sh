#!/bin/bash

# Point `chatgpt.cliExecutable` at this script in VS Code settings to debug the
# Codex extension with the latest Mikodex CLI binary.


set -euo pipefail

CODEX_RS_DIR=$(realpath "$(dirname "$0")/../codex-rs")
(cd "$CODEX_RS_DIR" && cargo run --quiet --bin mikodex -- "$@")
