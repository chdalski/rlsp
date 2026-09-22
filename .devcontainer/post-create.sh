#!/usr/bin/env bash

# Runs once after the container is created (postCreateCommand).
# Use this for one-time setup that only needs to happen on first build.

set -euo pipefail

# Fix volume ownership — Docker creates the pnpm store and shell history
# volumes as root, but pnpm and fish run as vscode and need write access.
sudo chown -R vscode:vscode /home/vscode/.local/share/pnpm /home/vscode/.local/share/fish

# Link Rust builds with mold (see cargo/config.toml). Copied here, not in the
# Dockerfile: CARGO_HOME only exists once the Rust feature has run, which is
# after the image build.
if [ -n "${CARGO_HOME:-}" ]; then
  cp /workspace/.devcontainer/cargo/config.toml "$CARGO_HOME/config.toml"
  echo "Copied cargo config to $CARGO_HOME/config.toml"
fi

# Language servers for the official TypeScript and Python code intelligence
# plugins (post-start.sh installs the ones the project enables); rust-analyzer
# comes from post-start.sh's setup_rust_analyzer step, not from the Rust
# feature, since that feature only ships rust-analyzer for the "stable"
# channel. Installed here because npm only exists once the Node feature has
# run.
if command -v npm >/dev/null; then
  npm install -g --no-fund --no-audit typescript typescript-language-server pyright
fi
