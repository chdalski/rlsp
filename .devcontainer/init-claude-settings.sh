#!/usr/bin/env bash

# Initialize Claude settings for devcontainer
# Reads host settings and modifies ANTHROPIC_BASE_URL to use external proxy

set -e

HOST_SETTINGS="/home/vscode/.claude-host-settings.json"
CONTAINER_SETTINGS="/home/vscode/.claude/settings.json"
PROXY_URL="${CLAUDE_PROXY_URL:-http://host.docker.internal:3000}"

echo "============================================================"
echo "Initializing Claude Settings"
echo "============================================================"

# Ensure .claude directory exists (volume should create it, but just in case)
mkdir -p /home/vscode/.claude

if [[ -f "$HOST_SETTINGS" ]]; then
    echo "Reading host settings from $HOST_SETTINGS"

    # Read host settings and override ANTHROPIC_BASE_URL
    jq --arg proxy_url "$PROXY_URL" '
        .env.ANTHROPIC_BASE_URL = $proxy_url
    ' "$HOST_SETTINGS" > "$CONTAINER_SETTINGS"

    echo "Written container settings to $CONTAINER_SETTINGS"
    echo "ANTHROPIC_BASE_URL set to: $PROXY_URL"
else
    echo "No host settings found at $HOST_SETTINGS"
    echo "Creating minimal settings with proxy URL"

    cat > "$CONTAINER_SETTINGS" <<EOF
{
  "env": {
    "ANTHROPIC_BASE_URL": "$PROXY_URL"
  }
}
EOF
fi

echo "============================================================"
echo "Claude settings initialized"
echo "============================================================"
