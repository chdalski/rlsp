# Devcontainer Template

Devcontainer template for sandboxed Claude Code agent execution, built on
Docker Compose.

## Setup

1. Copy the contents of this directory to your project's `.devcontainer/` folder
2. On macOS or Windows, remove `"docker-compose.audio.yml"` from
   `dockerComposeFile` in `devcontainer.json` — PulseAudio passthrough works
   on Linux only (see [Audio](#audio))
3. Optionally create `.devcontainer/.env` for personal settings and
   `.devcontainer/.env.credentials` for tokens (see
   [Configuration](#configuration))
4. Open your project in VS Code and use "Reopen in Container"

Requires Docker Compose 2.24 or newer (optional `env_file` entries).

## Files

| File | Purpose |
|------|---------|
| `devcontainer.json` | Features, named volumes, lifecycle scripts |
| `docker-compose.yml` | Build, resource limits, env files, read-only host binds |
| `docker-compose.audio.yml` | PulseAudio passthrough overlay (Linux) |
| `Dockerfile` | Base image and tools |
| `fish/config.fish` | Shell config; reads `.env.credentials` |
| `cargo/config.toml` | Links Rust builds with mold |
| `post-create.sh` | Once per container: volume ownership, cargo config, language servers |
| `post-start.sh` | Every start: Claude config, git identity, plugins, rust-analyzer |
| `.env.defaults` | Committed environment defaults |
| `.env.credentials.example` | Documents the credentials file format |

## Configuration

Three env files, each with one job:

| File | Committed | Read by | Holds | Takes effect |
|------|-----------|---------|-------|--------------|
| `.env.defaults` | yes | Compose | Team defaults (`CLAUDE_AUTH`, `GIT_EMAIL_DOMAIN`) | Rebuild Container |
| `.env` | no | Compose | Personal overrides, resource limits | Rebuild Container |
| `.env.credentials` | no | fish, at every shell start | Tokens and secrets | Next new shell |

`.env.defaults` and `.env` must never hold secrets: the devcontainer CLI
prints the resolved Compose configuration, values included, in its start
log. Tokens go in `.env.credentials`, which Compose never reads, so they stay
out of the start log and `docker inspect`. They are not hidden from the
agent — Claude runs in a shell with these variables exported. See
`.env.credentials.example` for the format.

Settings:

| Key | Default | Purpose |
|-----|---------|---------|
| `CLAUDE_AUTH` | `oauth` | Auth mode (see [Authentication Modes](#authentication-modes)) |
| `GIT_EMAIL_DOMAIN` | `chrisski.dev` | Mail domain of the random git identity for Claude's commits |
| `DEVCONTAINER_CPUS` | no limit | CPU cap (`.env` only, see [Resource limits](#resource-limits)) |
| `DEVCONTAINER_MEMORY` | no limit | Memory cap (`.env` only) |

Example `.env`:

```bash
CLAUDE_AUTH=proxy
DEVCONTAINER_CPUS=4
DEVCONTAINER_MEMORY=8g
```

### Resource limits

`DEVCONTAINER_CPUS` and `DEVCONTAINER_MEMORY` cap the container; unset or
`0` means no limit. They only work in `.devcontainer/.env` — Compose
substitutes them into `docker-compose.yml` from that file, not from
`.env.defaults`. For a team-wide default, change the fallback in
`docker-compose.yml` (e.g. `${DEVCONTAINER_CPUS:-4}`).

## Authentication Modes

The `CLAUDE_AUTH` environment variable controls how Claude Code authenticates.
`.env.defaults` sets `oauth`; override it in `.devcontainer/.env`.

| Mode | `CLAUDE_AUTH` | What gets copied | Use case |
|------|---------------|------------------|----------|
| OAuth (default) | `oauth` | `.credentials.json` + `settings.json` (env block and apiKeyHelper stripped) | Private Anthropic account |
| Proxy | `proxy` | `settings.json` (with env vars) | Work account via API proxy (e.g. Portkey) |

### OAuth mode (default)

Requires `~/.claude/.credentials.json` from a prior `claude login` on the
host. The script copies it into the container and strips the entire `env`
block and `apiKeyHelper` from `settings.json` so OAuth credentials are used
and no proxy config leaks in. Env vars needed in oauth mode (e.g.
`CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS`) go in `.devcontainer/.env`, or in
`.devcontainer/.env.credentials` if they are secret.

### Proxy mode

No extra setup needed. Your `~/.claude/settings.json` is copied as-is into
the container, including any `env` block with proxy configuration.

### Switching modes

Set `CLAUDE_AUTH` in `.devcontainer/.env`:

```bash
# .devcontainer/.env
CLAUDE_AUTH=proxy
```

After switching, delete the Claude config volume and rebuild the container
(see [Troubleshooting](#troubleshooting)).

## Audio

`docker-compose.audio.yml` passes the host's PulseAudio socket into the
container and sets `PULSE_SERVER`. `devcontainer.json` loads it by default.
The audio packages are always installed in the image; without the overlay
they do nothing.

### Linux

Works out of the box. The overlay bind-mounts `$XDG_RUNTIME_DIR/pulse/`
(fallback `/run/user/1000/pulse/`) into the container. PipeWire hosts work
too, through their PulseAudio socket.

### macOS (not tested)

Remove `docker-compose.audio.yml` from `dockerComposeFile` — the Linux socket
does not exist there. Install and start PulseAudio on the host with TCP:

```bash
brew install pulseaudio
pulseaudio --load=module-native-protocol-tcp --exit-idle-time=-1 --daemon
```

Then point the container at it in `.devcontainer/.env`:

```bash
PULSE_SERVER=docker.for.mac.localhost
```

You also need to share `~/.config/pulse` with the container for
authentication — add it to `volumes` in `docker-compose.yml`.

### Windows via WSL2 (not tested)

Requires Windows 11 with WSLg, which provides a PulseAudio server at
`/mnt/wslg/PulseServer`. Docker must run inside WSL2 (not Docker Desktop's
Windows containers mode). Does not work on Windows 10 or older setups
without WSLg.

Remove `docker-compose.audio.yml` from `dockerComposeFile`, set the server in
`.devcontainer/.env`:

```bash
PULSE_SERVER=/mnt/wslg/PulseServer
```

and add the WSLg mount to `volumes` in `docker-compose.yml`:

```yaml
      - /mnt/wslg:/mnt/wslg
```

## Volumes

Compose prefixes every named volume with its project name,
`<folder>_devcontainer`, where `<folder>` is the lowercased project folder
name. `<id>` is the `${devcontainerId}`: unique per checkout path and stable
across rebuilds.

| Volume | Target | Purpose |
|--------|--------|---------|
| `<folder>_devcontainer_claude-config-<id>` | `/home/vscode/.claude` | Claude config, sessions, memory, installed plugins |
| `<folder>_devcontainer_shell-history-<id>` | `/home/vscode/.local/share/fish` | fish history |
| `<folder>_devcontainer_pnpm-store-<id>` | `/home/vscode/.local/share/pnpm/store` | pnpm store |

Every checkout gets its own volumes, so worktrees and clones do not share
sessions or history. Moving or renaming a checkout changes its ID and leaves
the old volumes behind:

```bash
docker volume ls --filter name=_devcontainer_
docker volume rm <volume>
```

Two checkouts with the same folder name share one Compose project, so they
cannot run at the same time.

## Host Binds

| Source | Target | Purpose |
|--------|--------|---------|
| Project folder | `/workspace` | Workspace |
| `~/.claude/` (read-only) | `/home/vscode/.claude-host/` | Host config template directory |
| `~/.claude.json` (read-only) | `/home/vscode/.claude-host.json` | Host Claude config file (copied into container on startup) |
| `$XDG_RUNTIME_DIR/pulse/` | same path | PulseAudio socket (audio overlay) |

The read-only binds are declared in `docker-compose.yml`, not in
`devcontainer.json` `mounts`: on the Compose path the devcontainer CLI drops
the `readonly` flag from `mounts` entries. Missing sources fail the start
instead of being created on the host.

## Plugins and Language Servers

On every start, `post-start.sh` installs the plugins that the project's
`.claude/settings.json` enables (`enabledPlugins`, e.g. written by the
blueprints' `/project-init`) and that are not installed yet. Only entries
from `claude-plugins-official` are installed automatically — install plugins
from other marketplaces by hand. A failed install warns and the start
continues.

`post-create.sh` installs `typescript-language-server` and `pyright` for the
TypeScript and Python plugins. `post-start.sh` installs `rust-analyzer` for
the toolchain pinned in `rust-toolchain.toml` on every start — the Rust
feature only ships `rust-analyzer` for the `stable` channel, so `/workspace`,
which pins a specific version, needs it installed separately.

## Migrating from the `.env.local` Layout

- `.env.local` and `init-env` are gone. Move non-secret settings to `.env`
  and tokens to `.env.credentials`, then delete `.env.local` — new shells
  warn while it still exists. Do not just rename it: tokens in `.env` appear
  in the start log.
- All volumes are renamed, so the first start uses a fresh Claude config
  (host credentials are copied in again). To keep your sessions, copy the old
  volume into the new one while the devcontainer is stopped:

  ```bash
  docker volume ls   # old: claude-code-config-<folder>, new: <folder>_devcontainer_claude-config-<id>
  docker run --rm -v claude-code-config-<folder>:/from -v <new-volume>:/to alpine cp -a /from/. /to/
  ```

  Then remove the old `claude-code-config-*`, `claude-code-bashhistory-*`
  (always empty — fish never wrote to it) and `pnpm-store-*` volumes.

## Troubleshooting

### Auth errors or "Rate limit reached" after switching modes

The Claude config volume persists between container restarts. If you switch
authentication modes or change credentials, stale state in the volume can
cause unexpected errors (e.g. "Rate limit reached" that isn't a real rate
limit).

Fix: delete the volume and rebuild the container.

```bash
docker volume ls --filter name=claude-config
docker rm -f <folder>_devcontainer-sandbox-1   # a volume in use cannot be removed
docker volume rm <folder>_devcontainer_claude-config-<id>
```

### Claude Code ignores settings.json on first run

On fresh volumes, Claude Code >=2.0.65 may ignore `settings.json` and force
the login flow ([#13827](https://github.com/anthropics/claude-code/issues/13827)).
Deleting the volume and setting `CLAUDE_AUTH` before starting the container
resolves this.

### Start fails because a bind source path does not exist

A host path from [Host Binds](#host-binds) is missing. On macOS or Windows,
remove `docker-compose.audio.yml` from `dockerComposeFile`. If
`~/.claude.json` is missing, run `claude` once on the host.
