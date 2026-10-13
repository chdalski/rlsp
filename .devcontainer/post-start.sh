#!/usr/bin/env bash

# Runs on every container start (postStartCommand).
#
# 1. Copies host Claude settings into the container volume, once
# 2. Fills Claude Code's global config (.claude.json) on the volume, once:
#    from the container's ~/.claude.json, otherwise from the host copy.
#    CLAUDE_CONFIG_DIR (containerEnv in devcontainer.json) makes Claude Code
#    read it there, so onboarding and account state survive a rebuild
# 3. Updates Claude Code, so the first session starts on the latest release
# 4. Sets a random git identity for Claude commits (mail domain from
#    GIT_EMAIL_DOMAIN, default chrisski.dev)
# 5. Installs the official Claude plugins the project enables
# 6. Installs rust-analyzer for the toolchain pinned in rust-toolchain.toml
#
# Auth mode is controlled by the CLAUDE_AUTH environment variable:
#   - "oauth" (default): never copies the host's .credentials.json — each
#     container runs 'claude login' once and keeps its own OAuth session in
#     the ~/.claude volume. Copies settings.json with the entire env
#     block and apiKeyHelper removed — the env block typically contains
#     proxy config that conflicts with OAuth. Env vars needed in oauth
#     mode go in .devcontainer/.env (non-secret) or
#     .devcontainer/.env.credentials (tokens) instead.
#   - "proxy": copies settings.json from host, which contains API proxy
#     config (Portkey env vars, custom headers, etc.)
#
# The host copy runs only while the ~/.claude volume has no settings.json yet;
# afterwards the container keeps its own settings. To pick up host changes,
# delete ~/.claude/settings.json in the container (or the volume) and restart.
#
# CLAUDE_AUTH defaults to "oauth" in .devcontainer/.env.defaults. Override it
# in .devcontainer/.env (gitignored); changes take effect after "Rebuild
# Container".

set -euo pipefail

SEP="============================================================"

HOST_DIR="/home/vscode/.claude-host"
CONTAINER_DIR="/home/vscode/.claude"
HOST_CONFIG="/home/vscode/.claude-host.json"
CONTAINER_CONFIG="$CONTAINER_DIR/.claude.json"
# Where Claude Code kept its global config before CLAUDE_CONFIG_DIR was set;
# on the container filesystem, so a rebuild removes it.
LEGACY_CONFIG="/home/vscode/.claude.json"

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

  # ~/.claude is a persistent volume: once settings exist, keep them instead of
  # overwriting them with the host copy on every start.
  if [ -f "$CONTAINER_DIR/settings.json" ]; then
    echo "Container settings already present at $CONTAINER_DIR — skipping host copy."
    echo "$SEP"
    return 0
  fi

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
      # Never copy the host's .credentials.json: OAuth refresh tokens rotate,
      # so a copy shared by host and containers dies as soon as one of them
      # refreshes. Each container logs in once and keeps its own session.
      if [ ! -f "$CONTAINER_DIR/.credentials.json" ]; then
        echo "No container credentials yet (OAuth mode)."
        echo "Run 'claude login' once inside the container to authenticate."
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

  echo "Written container settings to $CONTAINER_DIR"
  echo "$SEP"
}

init_claude_config() {
  # CLAUDE_CONFIG_DIR (containerEnv) points Claude Code at the ~/.claude
  # volume, so its global config (onboarding, account, project settings) lives
  # there as .claude.json and survives a rebuild. Fill it once, then never
  # overwrite it. Runs before any `claude` call in this script: a call that
  # finds no config file creates an empty one, which this check would then
  # take as present.
  #
  # The file names account details, so it is written 0600 via a temp file in
  # the same directory (an interrupted copy leaves no truncated file that
  # counts as present), and only paths are logged, never its content.
  section "Initializing Claude Config"

  # A symlink counts as present, even a dangling one: never write through it.
  if [ -e "$CONTAINER_CONFIG" ] || [ -L "$CONTAINER_CONFIG" ]; then
    echo "Claude config already present at $CONTAINER_CONFIG — keeping it."
    echo "$SEP"
    return 0
  fi

  # The container's own file keeps the current state when an existing
  # container switches over; after a rebuild only the host copy is left.
  local src
  if [ -f "$LEGACY_CONFIG" ]; then
    src="$LEGACY_CONFIG"
  elif [ -f "$HOST_CONFIG" ]; then
    src="$HOST_CONFIG"
  else
    echo "No Claude config at $LEGACY_CONFIG or $HOST_CONFIG — Claude sets itself up on first use."
    echo "$SEP"
    return 0
  fi

  mkdir -p "$CONTAINER_DIR"
  local tmp
  if ! tmp="$(mktemp "$CONTAINER_CONFIG.XXXXXX")"; then
    echo "WARNING: could not create a temp file next to $CONTAINER_CONFIG"
    echo "$SEP"
    return 0
  fi
  if cp "$src" "$tmp" && chmod 600 "$tmp" && mv -fT "$tmp" "$CONTAINER_CONFIG"; then
    echo "Copied Claude config from $src to $CONTAINER_CONFIG"
  else
    rm -f "$tmp"
    echo "WARNING: could not copy Claude config from $src to $CONTAINER_CONFIG"
  fi
  echo "$SEP"
}

update_claude() {
  # Without this step the first session after a start finds the update
  # itself, installs it in the background and must be restarted to run it.
  # A rebuild needs the step as much as a restart: the image's install layer
  # comes from the build cache and keeps the release of the first build.
  #
  # The timeout bounds the start (VS Code waits for postStartCommand). A
  # failure only warns: the installed release keeps working, and the
  # session's own auto-update tries again.
  section "Updating Claude Code"
  if ! command -v claude >/dev/null; then
    echo "WARNING: claude is not on PATH, skipping update"
  elif ! timeout 120 claude update </dev/null; then
    echo "WARNING: claude update failed or timed out, keeping the installed release"
  fi
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
  # Before update_claude, the first phase that calls `claude`.
  init_claude_config
  update_claude
  init_git_identity
  setup_plugins
  setup_rust_analyzer
}

# Run only when executed, so the phases can be sourced and tested in isolation.
if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  main "$@"
fi
