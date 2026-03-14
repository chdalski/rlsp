#!/usr/bin/env bash

# Runs once after the container is created (postCreateCommand).
# Use this for one-time setup that only needs to happen on first build.

set -euo pipefail

# Fix pnpm store ownership — the volume is created as root by Docker,
# but pnpm runs as vscode and needs write access.
sudo chown -R vscode:vscode /home/vscode/.local/share/pnpm
