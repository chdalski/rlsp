#!/bin/sh
# SPDX-License-Identifier: MIT
#
# Provisions the `rlsp-yaml` binary into ${CLAUDE_PLUGIN_DATA} so `.lsp.json`
# (command: "${CLAUDE_PLUGIN_DATA}/rlsp-yaml") always has something to spawn,
# even on a machine with no `rlsp-yaml` on PATH. Runs as a SessionStart hook,
# so it must be safe to run on every session start: cheap no-op once a binary
# is in place, and it never blocks startup on failure (see the exit-0 note
# below).
#
# Every exit path in this script is `exit 0`. SessionStart hooks only ever
# surface stdout to Claude when they exit 0 (a non-zero exit here hides the
# guidance behind a stderr notice the user sees but Claude never does) — see
# the exit-code table in https://code.claude.com/docs/en/hooks. Since a
# missing/failed binary should be something Claude can explain to the user,
# every failure prints actionable guidance to stdout and exits 0 rather than
# signalling failure through the exit code. Nothing here treats a printed
# failure as a reason to leave a partially-provisioned binary behind, though:
# every failure path guarantees `${CLAUDE_PLUGIN_DATA}/rlsp-yaml` is either
# absent or the last-known-good binary, never a partial/corrupt one.
#
# Integrity: the tarball is downloaded from a pinned release tag and checked
# against a hardcoded sha256 recorded below, sourced from GitHub's own
# published asset digests (`gh release view <tag> --json assets`), not
# fetched over the network at provisioning time — an attacker able to swap
# the asset over the same channel could swap a fetched checksum file too.

set -eu

# Restricts everything provision.sh creates (the data dir, the staging dir,
# the eventual binary) to the current user. Without this, the two
# `mkdir -p "$CLAUDE_PLUGIN_DATA"` calls below inherit the ambient umask, and
# an unusually permissive one would leave the directory group/world-writable
# — under which Step 2's "already provisioned" reuse check (-f && -x, no
# re-verification) would trust whatever another local user substituted
# there. `mktemp -d` already ignores umask (always 0700); this closes the
# same gap for the plain `mkdir -p` calls.
umask 077

BINARY_NAME="rlsp-yaml"
PINNED_TAG="rlsp-yaml-v0.13.0"
GITHUB_RELEASE_PREFIX="https://github.com/chdalski/rlsp/releases/download/"
INSTALL_DOCS_URL="https://github.com/chdalski/rlsp/releases/${PINNED_TAG}"

: "${CLAUDE_PLUGIN_DATA:?CLAUDE_PLUGIN_DATA is not set}"

data_binary="${CLAUDE_PLUGIN_DATA}/${BINARY_NAME}"

guidance() {
    printf '%s\n' "rlsp-yaml: $1 Install rlsp-yaml manually from ${INSTALL_DOCS_URL} and put it on PATH."
}

require_tool() {
    if ! command -v "$1" >/dev/null 2>&1; then
        guidance "required tool '$1' is not available."
        exit 0
    fi
}

# Computes the sha256 digest of $1, printing only the hex digest. Prefers
# sha256sum (GNU coreutils, most Linux distros); falls back to shasum -a 256
# (macOS, BSD). Fails if neither is available.
compute_sha256() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{ print $1 }'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{ print $1 }'
    else
        return 1
    fi
}

# ---- Step 1: PATH-first -----------------------------------------------

path_binary=$(command -v "$BINARY_NAME" 2>/dev/null || true)
if [ -n "$path_binary" ] && [ -f "$path_binary" ] && [ -x "$path_binary" ]; then
    mkdir -p "$CLAUDE_PLUGIN_DATA"
    cp "$path_binary" "$data_binary"
    chmod 755 "$data_binary"
    exit 0
fi

# ---- Step 2: already provisioned ---------------------------------------

if [ -f "$data_binary" ] && [ -x "$data_binary" ]; then
    exit 0
fi

mkdir -p "$CLAUDE_PLUGIN_DATA"

# ---- Step 3: detect platform and map to a published release target -----

require_tool uname

os=$(uname -s)
arch=$(uname -m)

target=""
case "$os" in
    Linux)
        case "$arch" in
            x86_64) target="x86_64-unknown-linux-gnu" ;;
            aarch64) target="aarch64-unknown-linux-gnu" ;;
            riscv64) target="riscv64gc-unknown-linux-gnu" ;;
        esac
        ;;
    Darwin)
        case "$arch" in
            x86_64) target="x86_64-apple-darwin" ;;
            arm64) target="aarch64-apple-darwin" ;;
        esac
        ;;
esac

expected_sha256=""
case "$target" in
    x86_64-unknown-linux-gnu)
        expected_sha256="49a33187dcb015d2beaa11d27e2e0f2dc692e6229b075a9a590bfeb1ca8e17a9"
        ;;
    aarch64-unknown-linux-gnu)
        expected_sha256="cf45a0061275d1fe29095f7eb95a6a62f6d7a2879a298a31aff1b2d88bc4efb1"
        ;;
    riscv64gc-unknown-linux-gnu)
        expected_sha256="28649bb813365407fb7b5fbde419a64f4036947bfdbeb37c925843b1dd5212ad"
        ;;
    x86_64-apple-darwin)
        expected_sha256="1108ada0ce511aa790c8f43e1fc4dbdea5d3d79c972d5e8b22aa09db8cd43c66"
        ;;
    aarch64-apple-darwin)
        expected_sha256="118f9a40ce559ffa83417828312ead3b8c84b271f5bab7b039ad8ed870f79b00"
        ;;
esac

if [ -z "$target" ] || [ -z "$expected_sha256" ]; then
    guidance "unsupported platform (os=${os}, arch=${arch})."
    exit 0
fi

# ---- Step 4: download, verify, and extract ------------------------------

require_tool curl
require_tool tar
require_tool mktemp
require_tool chmod

if ! compute_sha256 /dev/null >/dev/null 2>&1; then
    guidance "no sha256 tool (sha256sum or shasum) is available, so a download cannot be verified."
    exit 0
fi

asset_name="rlsp-yaml-${target}.tar.gz"
url="${GITHUB_RELEASE_PREFIX}${PINNED_TAG}/${asset_name}"

# $url is always built from the constants above, so this can never take the
# `*)` arm today — it's a static invariant assertion, not a runtime control:
# a guard against a future change to the construction above (e.g. an
# env-var-overridable tag) silently starting to download from somewhere
# other than this project's own GitHub releases.
case "$url" in
    "${GITHUB_RELEASE_PREFIX}"*) ;;
    *)
        guidance "refused to download from an unexpected URL."
        exit 0
        ;;
esac

staging_dir=$(mktemp -d "${CLAUDE_PLUGIN_DATA}/.tmp.XXXXXX")
trap 'rm -rf "$staging_dir"' EXIT

archive="${staging_dir}/${asset_name}"

if ! curl --fail --silent --show-error --location \
    --proto '=https' --proto-redir '=https' --max-time 120 --max-filesize 104857600 \
    --output "$archive" "$url"; then
    guidance "the download of ${asset_name} failed (network or transport error)."
    exit 0
fi

actual_sha256=$(compute_sha256 "$archive" | tr '[:upper:]' '[:lower:]')
expected_sha256_lc=$(printf '%s' "$expected_sha256" | tr '[:upper:]' '[:lower:]')

if [ "$actual_sha256" != "$expected_sha256_lc" ]; then
    rm -f "$archive"
    guidance "the download of ${asset_name} failed an integrity (checksum) check."
    exit 0
fi

# Two listings of the same archive: `tzf` gives exact entry names (one per
# line, spaces and all — safe against a crafted name like "../x rlsp-yaml"
# that would otherwise slip past a whitespace-split name check), `tvzf`
# gives the leading type character ('-' for a plain file, 'l' for a
# symlink, etc). A `tar` invocation failing here (corrupt archive, or no
# working `tar` at all) is reported the same way as an extraction failure
# below — from the outside both mean "tar could not produce a binary".
plain_listing="${staging_dir}/listing.txt"
verbose_listing="${staging_dir}/listing_verbose.txt"
if ! tar tzf "$archive" >"$plain_listing" 2>/dev/null \
    || ! tar tvzf "$archive" >"$verbose_listing" 2>/dev/null; then
    guidance "failed to extract ${asset_name} (could not read its contents)."
    exit 0
fi

entry_count=$(wc -l <"$plain_listing" | tr -d ' ')
if [ "$entry_count" -ne 1 ]; then
    guidance "${asset_name} has unexpected contents and was not extracted."
    exit 0
fi

entry_name=$(cat "$plain_listing")
entry_type=$(cut -c1 "$verbose_listing")

if [ "$entry_type" != "-" ] || [ "$entry_name" != "$BINARY_NAME" ]; then
    guidance "${asset_name} has unexpected contents and was not extracted."
    exit 0
fi

extract_dir="${staging_dir}/extracted"
mkdir -p "$extract_dir"
if ! tar xzf "$archive" -C "$extract_dir" --no-same-owner --no-same-permissions; then
    guidance "failed to extract ${asset_name}."
    exit 0
fi

extracted_binary="${extract_dir}/${BINARY_NAME}"
extracted_count=$(find "$extract_dir" -mindepth 1 | wc -l | tr -d ' ')

if [ "$extracted_count" -ne 1 ] || [ -L "$extracted_binary" ] || [ ! -f "$extracted_binary" ]; then
    guidance "extracting ${asset_name} did not produce the expected binary."
    exit 0
fi

chmod 755 "$extracted_binary"
mv "$extracted_binary" "$data_binary"

exit 0
