#!/usr/bin/env bash

# DevContainer postStartCommand
# This runs every time the container starts

set -e

echo "============================================================"
echo "DevContainer Post-Start Setup"
echo "============================================================"

# 1. Install mitmproxy if not already installed
if ! command -v mitmdump &> /dev/null; then
    echo "📦 Installing mitmproxy..."
    pip3 install --user mitmproxy
fi

# Start cache proxy as a completely detached daemon process
# This ensures it survives after the parent script exits

cd /workspace/.devcontainer/claude_proxy

# Kill any existing proxy
pkill -f "python.*cache-proxy.py" 2>/dev/null || true
pkill -f "mitmdump" 2>/dev/null || true
sleep 1

# Make script executable
chmod +x cache-proxy.py

# Start proxy using setsid to create new session (completely detached)
setsid python3 cache-proxy.py > /workspace/cache-proxy.log 2>&1 < /dev/null &

# Wait for startup
sleep 3

# Verify it started by checking for the startup banner in the log
if grep -q "Prompt Caching Proxy Started" /workspace/cache-proxy.log; then
    echo "✓ Cache proxy started successfully on port 3000"
    echo "  Check logs: tail -f /workspace/cache-proxy.log"
else
    echo "✗ Cache proxy failed to start"
    tail -20 /workspace/cache-proxy.log
    exit 1
fi