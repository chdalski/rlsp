# Read .devcontainer/.env.credentials (gitignored) into the environment of
# every new shell. Parsed as KEY=value, never sourced: the value is
# everything after the first `=`, taken literally (no quote stripping, no $
# or (...) expansion). A missing file is silent; malformed lines warn by line
# number only, never with the value. See .env.credentials.example.
if test -f /workspace/.devcontainer/.env.credentials
    set -l line_no 0
    while read -l line
        set line_no (math $line_no + 1)
        set line (string replace -r '\r$' '' -- $line)
        # Strip a UTF-8 BOM, if present — only ever matches at the start of
        # line 1, same as `docker --env-file`.
        set line (string replace -r '^\x{FEFF}' '' -- $line)
        set -l trimmed (string trim -l -- $line)
        if test -z "$trimmed"; or string match -q '#*' -- $trimmed
            continue
        end
        if not string match -q '*=*' -- $trimmed
            echo "/workspace/.devcontainer/.env.credentials:$line_no: skipping line without '='" >&2
            continue
        end
        set -l parts (string split -m 1 '=' -- $trimmed)
        set -l key $parts[1]
        set -l value $parts[2]
        if not string match -qr '^[A-Za-z_][A-Za-z0-9_]*$' -- $key
            echo "/workspace/.devcontainer/.env.credentials:$line_no: skipping invalid key" >&2
            continue
        end
        set -gx -- $key $value
    end </workspace/.devcontainer/.env.credentials
end

# .env.local was replaced by .env (non-secret) and .env.credentials (tokens)
# and is no longer read. Warn while it is still around, so its settings don't
# look silently applied. Never print its contents.
if status is-interactive; and test -f /workspace/.devcontainer/.env.local
    set_color yellow
    echo "WARNING: .devcontainer/.env.local exists but is no longer read." >&2
    echo "         Move non-secret settings to .devcontainer/.env and tokens to" >&2
    echo "         .devcontainer/.env.credentials, delete .env.local, then" >&2
    echo "         Rebuild Container. Tokens never go in .env — its values" >&2
    echo "         appear in the start log. See .devcontainer/README.md." >&2
    set_color normal
end

fish_add_path /home/vscode/.local/bin
alias l="ls -lisa"
alias claude="claude --dangerously-skip-permissions"
starship init fish | source
