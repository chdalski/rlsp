@echo off
rem Creates .env.local with default auth mode if it doesn't exist yet.
rem Called via initializeCommand before the container starts.
rem See init-env (no extension) for the Unix equivalent.
if not exist .devcontainer\.env.local echo CLAUDE_AUTH=proxy> .devcontainer\.env.local
