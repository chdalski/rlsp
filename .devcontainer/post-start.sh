#!/usr/bin/env bash

# Runs on every container start (postStartCommand).
#
# 1. Copies host Claude configuration into the container volume
# 2. Sets a random git identity for Claude commits (mail domain from
#    GIT_EMAIL_DOMAIN, default chrisski.dev)
# 3. Installs the official Claude plugins the project enables
# 4. Installs rust-analyzer for the toolchain pinned in rust-toolchain.toml
#
# Auth mode is controlled by the CLAUDE_AUTH environment variable:
#   - "oauth" (default): copies .credentials.json from host, which contains
#     Anthropic OAuth tokens. Copies settings.json with the entire env
#     block and apiKeyHelper removed — the env block typically contains
#     proxy config that conflicts with OAuth. Env vars needed in oauth
#     mode go in .devcontainer/.env (non-secret) or
#     .devcontainer/.env.credentials (tokens) instead.
#   - "proxy": copies settings.json from host, which contains API proxy
#     config (Portkey env vars, custom headers, etc.)
#
# CLAUDE_AUTH defaults to "oauth" in .devcontainer/.env.defaults. Override it
# in .devcontainer/.env (gitignored); changes take effect after "Rebuild
# Container".

set -euo pipefail

SEP="============================================================"

HOST_DIR="/home/vscode/.claude-host"
CONTAINER_DIR="/home/vscode/.claude"
HOST_CONFIG="/home/vscode/.claude-host.json"

PROJECT_SETTINGS="/workspace/.claude/settings.json"
OFFICIAL_MARKETPLACE="claude-plugins-official"
# Full https URL, never the owner/repo short form: with a forwarded SSH agent
# the CLI clones the short form over SSH, which puts the agent on the
# unattended start path and can stall on a passphrase prompt.
OFFICIAL_MARKETPLACE_URL="https://github.com/anthropics/claude-plugins-official.git"

section() {
  echo "$SEP"
  echo "$1"
  echo "$SEP"
}

init_claude_settings() {
  : "${CLAUDE_AUTH:=oauth}"

  section "Initializing Claude Settings (auth mode: $CLAUDE_AUTH)"

  mkdir -p "$CONTAINER_DIR"

  case "$CLAUDE_AUTH" in
    proxy)
      HOST_SETTINGS="$HOST_DIR/settings.json"
      if [ -f "$HOST_SETTINGS" ]; then
        echo "Copying host settings.json (proxy mode)"
        cp "$HOST_SETTINGS" "$CONTAINER_DIR/settings.json"
      else
        echo "WARNING: No settings.json found at $HOST_SETTINGS"
        echo '{}' > "$CONTAINER_DIR/settings.json"
      fi
      ;;
    oauth)
      HOST_CREDENTIALS="$HOST_DIR/.credentials.json"
      if [ -f "$HOST_CREDENTIALS" ]; then
        echo "Copying host .credentials.json (OAuth mode)"
        cp "$HOST_CREDENTIALS" "$CONTAINER_DIR/.credentials.json"
      else
        echo "WARNING: No .credentials.json found at $HOST_CREDENTIALS"
        echo "Run 'claude login' inside the container to authenticate."
      fi
      # Copy settings.json if it exists, but strip the entire env block
      # and apiKeyHelper — the env block contains proxy config that conflicts
      # with OAuth. Env vars needed in oauth mode go in .env or
      # .env.credentials.
      HOST_SETTINGS="$HOST_DIR/settings.json"
      if [ -f "$HOST_SETTINGS" ]; then
        echo "Copying host settings.json (stripping env block and apiKeyHelper)"
        if command -v jq &>/dev/null; then
          jq 'del(.apiKeyHelper, .env)' "$HOST_SETTINGS" > "$CONTAINER_DIR/settings.json"
        else
          echo "WARNING: jq not available, copying settings.json as-is"
          echo "Proxy env vars may override OAuth credentials."
          cp "$HOST_SETTINGS" "$CONTAINER_DIR/settings.json"
        fi
      else
        echo '{}' > "$CONTAINER_DIR/settings.json"
      fi
      ;;
    *)
      echo "ERROR: Unknown CLAUDE_AUTH value: $CLAUDE_AUTH"
      echo "Valid values: proxy, oauth"
      exit 1
      ;;
  esac

  if [ -f "$HOST_CONFIG" ]; then
    echo "Copying host .claude.json"
    cp "$HOST_CONFIG" "/home/vscode/.claude.json"
  else
    echo "WARNING: No .claude.json found at $HOST_CONFIG"
  fi

  echo "Written container settings to $CONTAINER_DIR"
  echo "$SEP"
}

init_git_identity() {
  section "Initializing Git Identity"

  NAMES=("Claus Coder" "Claudia Coder" "Mr. Robot" "Mrs. Robot")
  SELECTED="${NAMES[$((RANDOM % ${#NAMES[@]}))]}"

  FIRST=$(echo "$SELECTED" | awk '{print $1}' | tr '[:upper:]' '[:lower:]' | tr -cd 'a-z0-9')
  SECOND=$(echo "$SELECTED" | awk '{print $2}' | tr '[:upper:]' '[:lower:]' | tr -cd 'a-z0-9')
  EMAIL="${FIRST}.${SECOND}@${GIT_EMAIL_DOMAIN:-chrisski.dev}"

  git config --global user.name "$SELECTED"
  git config --global user.email "$EMAIL"

  echo "Git identity set: $SELECTED <$EMAIL>"
  echo "$SEP"
}

setup_plugins() {
  # Install the official plugins the project enables. Enabling lives in the
  # committed .claude/settings.json (e.g. written by /project-init), but
  # install state lives in the claude-config volume, which starts empty for
  # every checkout — without this step Claude reports the plugins as
  # "enabled but not installed".
  #
  # Only @claude-plugins-official entries: whoever can write settings.json
  # decides what this unattended step installs, so plugins from other
  # marketplaces stay a manual install. No -y / --accept-command either: a
  # plugin that declares an install command is refused instead of running
  # unattended. Every failure warns and lets the start continue — plugins
  # are not on Claude's request path.
  [ -f "$PROJECT_SETTINGS" ] || return 0

  local plugins
  if ! plugins="$(jq -r --arg m "@$OFFICIAL_MARKETPLACE" \
    '.enabledPlugins // {} | to_entries[]
     | select(.value == true and (.key | endswith($m))) | .key' \
    "$PROJECT_SETTINGS")"; then
    echo "WARNING: could not parse $PROJECT_SETTINGS, skipping plugin install"
    return 0
  fi
  [ -n "$plugins" ] || return 0

  section "Installing Claude plugins"

  if ! command -v claude >/dev/null; then
    echo "WARNING: claude is not on PATH, skipping plugin install"
    echo "$SEP"
    return 0
  fi

  # A fresh volume has no marketplace registered yet. GIT_TERMINAL_PROMPT=0
  # makes a credential prompt fail fast instead of blocking the start.
  if ! claude plugin marketplace list --json 2>/dev/null |
    jq -e --arg n "$OFFICIAL_MARKETPLACE" 'any(.[]; .name == $n)' >/dev/null; then
    if ! GIT_TERMINAL_PROMPT=0 claude plugin marketplace add "$OFFICIAL_MARKETPLACE_URL"; then
      echo "WARNING: could not add marketplace $OFFICIAL_MARKETPLACE, skipping plugin install"
      echo "$SEP"
      return 0
    fi
  fi

  local installed plugin
  local wanted=() failed=()
  mapfile -t wanted <<<"$plugins"
  installed="$(claude plugin list --json 2>/dev/null | jq -r '.[].id')" || installed=""
  for plugin in "${wanted[@]}"; do
    if grep -Fxq "$plugin" <<<"$installed"; then
      echo "$plugin already installed, skipping."
    elif claude plugin install "$plugin"; then
      echo "Installed $plugin"
    else
      echo "WARNING: failed to install $plugin"
      failed+=("$plugin")
    fi
  done

  # One summary line, so a start that quietly skipped a plugin does not look
  # like a healthy one in the postStart log.
  if [ "${#failed[@]}" -gt 0 ]; then
    echo "WARNING: ${#failed[@]} plugin(s) not installed: ${failed[*]}"
    echo "         Claude will report them as enabled but not installed."
  fi

  echo "$SEP"
}

setup_rust_analyzer() {
  # The Rust devcontainer feature only installs rust-analyzer for the
  # "stable" channel, so inside /workspace — where rust-toolchain.toml pins
  # a specific version — the rustup proxy fails with "error: Unknown binary
  # 'rust-analyzer' in official toolchain '<version>-...'". Installs the
  # component for whatever toolchain is actually pinned, so a version bump
  # in rust-toolchain.toml is picked up at the next container start with no
  # change needed here. Every failure warns and lets the start continue —
  # rust-analyzer is not on Claude's request path.
  section "Installing rust-analyzer for the pinned toolchain"

  if ! command -v rustup >/dev/null; then
    echo "WARNING: rustup is not on PATH, skipping rust-analyzer install"
    echo "$SEP"
    return 0
  fi

  # Install the toolchain rust-toolchain.toml pins, with the components it
  # declares (clippy, rustfmt) — explicit, not rustup's install-on-first-use
  # auto-install, which is deprecated (rust-lang/rustup#4836) and warns "may
  # stop working in the future". Running with no toolchain argument, from
  # /workspace, makes rustup resolve and install the pin from the override
  # file itself — no parsing of rust-toolchain.toml here. On a fresh
  # container, this is also what first makes the pinned toolchain installed
  # at all; on one that already has it, it is a no-op.
  if ! (cd /workspace && rustup toolchain install --no-self-update); then
    echo "WARNING: could not install the toolchain pinned in /workspace, skipping rust-analyzer install"
    echo "$SEP"
    return 0
  fi

  # Ask rustup which toolchain that resolved to, so the component install
  # below names it explicitly and does not depend on cwd.
  local toolchain
  if ! toolchain="$(cd /workspace && rustup show active-toolchain 2>/dev/null | awk '{print $1}')" ||
    [ -z "$toolchain" ]; then
    echo "WARNING: could not resolve the toolchain pinned in /workspace, skipping rust-analyzer install"
    echo "$SEP"
    return 0
  fi

  if rustup component add --toolchain "$toolchain" rust-analyzer; then
    echo "rust-analyzer installed for $toolchain"
  else
    echo "WARNING: failed to install rust-analyzer for $toolchain"
  fi

  echo "$SEP"
}

main() {
  init_claude_settings
  init_git_identity
  setup_plugins
  setup_rust_analyzer
}

main "$@"
