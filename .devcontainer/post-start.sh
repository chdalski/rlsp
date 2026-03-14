#!/usr/bin/env bash

# Copy host Claude configuration into the container volume.
#
# Auth mode is controlled by the CLAUDE_AUTH environment variable:
#   - "proxy"  (default): copies settings.json from host, which contains
#     API proxy config (Portkey env vars, custom headers, etc.)
#   - "oauth": copies .credentials.json from host, which contains
#     Anthropic OAuth tokens. Copies settings.json with the entire env
#     block and apiKeyHelper removed — the env block typically contains
#     proxy config that conflicts with OAuth. Any env vars needed in
#     oauth mode should be added to .devcontainer/.env.local instead.
#
# CLAUDE_AUTH is set via containerEnv in devcontainer.json (default: proxy).
# Override locally via .devcontainer/.env.local (gitignored).

set -euo pipefail

SEP="============================================================"

if [ -z "${CLAUDE_AUTH:-}" ]; then
  echo "ERROR: CLAUDE_AUTH is not set. Set it in devcontainer.json containerEnv"
  echo "or in .devcontainer/.env.local. Valid values: proxy, oauth"
  exit 1
fi
HOST_DIR="/home/vscode/.claude-host"
CONTAINER_DIR="/home/vscode/.claude"

echo "$SEP"
echo "Initializing Claude Settings (auth mode: $CLAUDE_AUTH)"
echo "$SEP"

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
    # with OAuth. Any env vars needed in oauth mode go in .env.local.
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

HOST_CONFIG="/home/vscode/.claude-host.json"
if [ -f "$HOST_CONFIG" ]; then
  echo "Copying host .claude.json"
  cp "$HOST_CONFIG" "/home/vscode/.claude.json"
else
  echo "WARNING: No .claude.json found at $HOST_CONFIG"
fi

echo "Written container settings to $CONTAINER_DIR"
echo "$SEP"

echo "$SEP"
echo "Initializing Git Identity"
echo "$SEP"

NAMES=("Claus Coder" "Claudia Coder" "Mr. Robot" "Mrs. Robot")
SELECTED="${NAMES[$((RANDOM % ${#NAMES[@]}))]}"

FIRST=$(echo "$SELECTED" | awk '{print $1}' | tr '[:upper:]' '[:lower:]' | tr -cd 'a-z0-9')
SECOND=$(echo "$SELECTED" | awk '{print $2}' | tr '[:upper:]' '[:lower:]' | tr -cd 'a-z0-9')
EMAIL="${FIRST}.${SECOND}@codecentric.de"

git config --global user.name "$SELECTED"
git config --global user.email "$EMAIL"

echo "Git identity set: $SELECTED <$EMAIL>"
echo "$SEP"
