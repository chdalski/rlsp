#!/bin/sh
# SPDX-License-Identifier: MIT
#
# Hermetic test suite for provision.sh. Run:
#
#   sh provision.test.sh
#
# Every test runs provision.sh as a real subprocess (never sourced) inside
# an `env -i` sandbox: a fully-curated PATH (the real POSIX utilities the
# script needs, resolved once from the host at suite start) plus a fresh,
# per-test CLAUDE_PLUGIN_DATA. curl, uname, and sha256sum are swappable per
# test between "real" (a symlink into the curated PATH) and "stub" (a small
# script that reads *_STUB_* env vars at run time) — see build_toolkit_dir,
# stub_uname, stub_curl, stub_sha256sum. Because provision.sh's every exit
# path is `exit 0` (see its own header comment), most assertions here check
# side effects — is the binary present? did curl get invoked? what did
# stdout say? — rather than the exit code.
#
# Real-network smoke test: the suite also includes one test that runs the
# unstubbed script against the actual pinned GitHub release. It is skipped
# by default (keeps the suite hermetic/CI-safe) and opts in via:
#
#   PROVISION_TEST_NETWORK=1 sh provision.test.sh

set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
PROVISION_SH="${SCRIPT_DIR}/provision.sh"
SH_BIN=$(command -v sh)

if [ ! -f "$PROVISION_SH" ]; then
    printf 'provision.sh not found at %s\n' "$PROVISION_SH" >&2
    exit 1
fi

SUITE_ROOT=$(mktemp -d "${TMPDIR:-/tmp}/provision-test.XXXXXX")
trap 'rm -rf "$SUITE_ROOT"' EXIT

# ---- toolkit: real utilities provision.sh needs, resolved from the host's
# real PATH once, before any test replaces PATH -----------------------------

TOOLKIT_TOOLS="mkdir cp chmod uname awk tr mktemp curl tar gzip wc cut find mv rm cat sha256sum shasum"

TOOLKIT_DIR="${SUITE_ROOT}/toolkit"
mkdir -p "$TOOLKIT_DIR"
for tool in $TOOLKIT_TOOLS; do
    tool_path=$(command -v "$tool" 2>/dev/null || true)
    if [ -n "$tool_path" ]; then
        ln -s "$tool_path" "${TOOLKIT_DIR}/${tool}"
    fi
done

# Builds a curated PATH dir at $1, containing every $TOOLKIT_TOOLS entry
# except any name(s) listed (space-separated) in $2 — used to simulate
# "tool not installed" for require_tool tests.
new_toolkit() {
    dir="$1"
    omit="$2"
    mkdir -p "$dir"
    for tool in $TOOLKIT_TOOLS; do
        case " $omit " in
            *" $tool "*) continue ;;
        esac
        if [ -e "${TOOLKIT_DIR}/${tool}" ]; then
            ln -s "${TOOLKIT_DIR}/${tool}" "${dir}/${tool}"
        fi
    done
}

# ---- stubs: replace a curated-PATH entry with a script controlled by
# *_STUB_* env vars read at run time (set by the calling test, forwarded by
# run()) ---------------------------------------------------------------------

stub_uname() {
    dir="$1"
    rm -f "${dir}/uname"
    cat >"${dir}/uname" <<'EOF'
#!/bin/sh
case "$1" in
    -s) printf '%s\n' "${UNAME_STUB_OS:?UNAME_STUB_OS not set}" ;;
    -m) printf '%s\n' "${UNAME_STUB_ARCH:?UNAME_STUB_ARCH not set}" ;;
    *) exit 1 ;;
esac
EOF
    chmod 755 "${dir}/uname"
}

# Records every invocation to CURL_STUB_LOG (so tests can assert whether a
# download was attempted, and which URL was requested), then either fails
# (CURL_STUB_MODE=fail, the default — simulates "no network reachable" for
# tests that only care whether curl was called) or copies CURL_STUB_FIXTURE
# to the `--output` path (CURL_STUB_MODE=copy — simulates a successful
# download of a locally-built fixture tarball).
stub_curl() {
    dir="$1"
    rm -f "${dir}/curl"
    cat >"${dir}/curl" <<'EOF'
#!/bin/sh
: "${CURL_STUB_LOG:?CURL_STUB_LOG not set}"
printf '%s\n' "$*" >>"$CURL_STUB_LOG"
if [ "${CURL_STUB_MODE:-fail}" = "fail" ]; then
    exit 1
fi
out=""
prev=""
for arg in "$@"; do
    if [ "$prev" = "--output" ]; then
        out="$arg"
    fi
    prev="$arg"
done
[ -n "$out" ] || exit 1
cp "${CURL_STUB_FIXTURE:?CURL_STUB_FIXTURE not set}" "$out"
EOF
    chmod 755 "${dir}/curl"
}

# Ignores the file it's asked to hash and always reports a fixed digest:
# SHA_STUB_EXPECTED verbatim in "match" mode (the default), or a fixed wrong
# 64-hex-char digest in "mismatch" mode. compute_sha256() in provision.sh
# only ever consumes the first awk field, so the trailing filename here is
# cosmetic (matches real sha256sum's "<hash>  <path>" output shape).
stub_sha256sum() {
    dir="$1"
    rm -f "${dir}/sha256sum" "${dir}/shasum"
    cat >"${dir}/sha256sum" <<'EOF'
#!/bin/sh
if [ "${SHA_STUB_MODE:-match}" = "match" ]; then
    printf '%s  %s\n' "${SHA_STUB_EXPECTED:?SHA_STUB_EXPECTED not set}" "$1"
else
    printf '%s  %s\n' "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff" "$1"
fi
EOF
    chmod 755 "${dir}/sha256sum"
}

# ---- fixture tarballs: built with the real system tar, never downloaded --

fixture_valid_tarball() {
    out="$1"
    content="$2"
    stage="${TEST_TMP}/stage"
    rm -rf "$stage"
    mkdir -p "$stage"
    printf '%s' "$content" >"${stage}/rlsp-yaml"
    ( cd "$stage" && tar czf "$out" rlsp-yaml )
}

fixture_wrong_name_tarball() {
    out="$1"
    stage="${TEST_TMP}/stage"
    rm -rf "$stage"
    mkdir -p "$stage"
    printf 'binary\n' >"${stage}/payload"
    ( cd "$stage" && tar czf "$out" payload )
}

fixture_multi_entry_tarball() {
    out="$1"
    stage="${TEST_TMP}/stage"
    rm -rf "$stage"
    mkdir -p "$stage"
    printf 'binary\n' >"${stage}/rlsp-yaml"
    printf 'decoy\n' >"${stage}/decoy"
    ( cd "$stage" && tar czf "$out" rlsp-yaml decoy )
}

fixture_empty_tarball() {
    out="$1"
    tar czf "$out" -T /dev/null
}

fixture_symlink_tarball() {
    out="$1"
    stage="${TEST_TMP}/stage"
    rm -rf "$stage"
    mkdir -p "$stage"
    ln -s "/etc/passwd" "${stage}/rlsp-yaml"
    ( cd "$stage" && tar czf "$out" rlsp-yaml )
}

# A single regular file, renamed inside the archive to a name containing a
# space: "../x rlsp-yaml". `awk '{ print $NF }'` (the old, replaced parsing
# approach) would read this entry's last whitespace-separated field as
# "rlsp-yaml" and wrongly accept it; the current entry_name=$(cat ...)
# (whole-line, exact match) correctly rejects it. See provision.sh's own
# comment at the tar-listing step.
fixture_traversal_named_tarball() {
    out="$1"
    stage="${TEST_TMP}/stage"
    rm -rf "$stage"
    mkdir -p "$stage"
    printf 'binary\n' >"${stage}/rlsp-yaml"
    ( cd "$stage" && tar --transform 's,^rlsp-yaml$,../x rlsp-yaml,' -czf "$out" rlsp-yaml 2>/dev/null )
}

fixture_garbage_file() {
    out="$1"
    printf 'not a tarball, just bytes\n' >"$out"
}

# ---- assertions -------------------------------------------------------

fail() {
    printf 'FAIL: %s: %s\n' "$CURRENT_TEST" "$1" >&2
    exit 1
}

assert_eq() {
    if [ "$1" != "$2" ]; then
        fail "expected [$2], got [$1]${3:+ ($3)}"
    fi
}

assert_file_exists() {
    if [ ! -f "$1" ]; then
        fail "expected file to exist: $1"
    fi
}

assert_file_absent() {
    if [ -e "$1" ]; then
        fail "expected file to be absent: $1"
    fi
}

assert_executable() {
    if [ ! -x "$1" ]; then
        fail "expected file to be executable: $1"
    fi
}

assert_stdout_contains() {
    if ! grep -q -- "$1" "$STDOUT_FILE"; then
        fail "expected stdout to contain [$1], got: $(cat "$STDOUT_FILE")"
    fi
}

assert_not_invoked() {
    if [ -e "$1" ]; then
        fail "expected $2 to not be invoked, but it was (log: $1)"
    fi
}

assert_invoked() {
    if [ ! -e "$1" ]; then
        fail "expected $2 to be invoked, but it wasn't"
    fi
}

# Nothing at all in the data dir — no binary, and no leftover .tmp.*
# staging dir (the trap in provision.sh should always clean that up, on
# both success and every rejection path).
assert_data_dir_empty() {
    if [ -d "$1" ]; then
        count=$(find "$1" -mindepth 1 -maxdepth 1 | wc -l | tr -d ' ')
        if [ "$count" -ne 0 ]; then
            fail "expected $1 to be empty, found: $(ls -A "$1")"
        fi
    fi
}

assert_data_dir_contains_only_binary() {
    count=$(find "$1" -mindepth 1 -maxdepth 1 | wc -l | tr -d ' ')
    if [ "$count" -ne 1 ]; then
        fail "expected only the provisioned binary in $1, found: $(ls -A "$1")"
    fi
}

# ---- per-test sandbox ---------------------------------------------------

# Sets up a fresh, isolated sandbox for one test: TEST_TMP, PATH_DIR (a
# curated toolkit, optionally omitting the space-separated tool names in
# $1), DATA_DIR (deliberately not created — exercises provision.sh's own
# `mkdir -p`), STDOUT_FILE, STDERR_FILE.
setup_test() {
    TEST_TMP=$(mktemp -d "${SUITE_ROOT}/case.XXXXXX")
    PATH_DIR="${TEST_TMP}/bin"
    DATA_DIR="${TEST_TMP}/data"
    STDOUT_FILE="${TEST_TMP}/stdout"
    STDERR_FILE="${TEST_TMP}/stderr"
    CURL_STUB_MODE=""
    CURL_STUB_FIXTURE=""
    CURL_STUB_LOG=""
    SHA_STUB_MODE=""
    SHA_STUB_EXPECTED=""
    UNAME_STUB_OS=""
    UNAME_STUB_ARCH=""
    new_toolkit "$PATH_DIR" "${1:-}"
}

# Runs provision.sh in the sandbox built by setup_test(). Any *_STUB_* vars
# a test set beforehand are forwarded; unset ones fall back to harmless
# defaults (curl stub, if installed, defaults to "fail" — no network).
run() {
    set +e
    env -i \
        PATH="$PATH_DIR" \
        CLAUDE_PLUGIN_DATA="$DATA_DIR" \
        CURL_STUB_MODE="${CURL_STUB_MODE:-fail}" \
        CURL_STUB_FIXTURE="${CURL_STUB_FIXTURE:-}" \
        CURL_STUB_LOG="${CURL_STUB_LOG:-${TEST_TMP}/curl.log}" \
        SHA_STUB_MODE="${SHA_STUB_MODE:-match}" \
        SHA_STUB_EXPECTED="${SHA_STUB_EXPECTED:-}" \
        UNAME_STUB_OS="${UNAME_STUB_OS:-}" \
        UNAME_STUB_ARCH="${UNAME_STUB_ARCH:-}" \
        "$SH_BIN" "$PROVISION_SH" >"$STDOUT_FILE" 2>"$STDERR_FILE"
    RUN_EXIT=$?
    set -e
}

# ==== Boundary / contract ==================================================

test_missing_claude_plugin_data_exits_nonzero() {
    setup_test
    set +e
    env -i PATH="$PATH_DIR" "$SH_BIN" "$PROVISION_SH" >"$STDOUT_FILE" 2>"$STDERR_FILE"
    RUN_EXIT=$?
    set -e
    if [ "$RUN_EXIT" -eq 0 ]; then
        fail "expected a non-zero exit, got 0"
    fi
    if ! grep -q "CLAUDE_PLUGIN_DATA" "$STDERR_FILE"; then
        fail "expected stderr to mention CLAUDE_PLUGIN_DATA, got: $(cat "$STDERR_FILE")"
    fi
}

# ==== PATH-first ============================================================

test_path_binary_present_is_copied_and_reused() {
    setup_test
    path_binary_content="real path binary bytes"
    printf '%s' "$path_binary_content" >"${PATH_DIR}/rlsp-yaml"
    chmod 755 "${PATH_DIR}/rlsp-yaml"

    stub_curl "$PATH_DIR"
    CURL_STUB_LOG="${TEST_TMP}/curl.log"
    run

    assert_eq "$RUN_EXIT" "0" "exit code"
    assert_file_exists "${DATA_DIR}/rlsp-yaml"
    assert_executable "${DATA_DIR}/rlsp-yaml"
    assert_eq "$(cat "${DATA_DIR}/rlsp-yaml")" "$path_binary_content" "data-dir binary content"
    assert_not_invoked "$CURL_STUB_LOG" "curl"
}

test_path_binary_non_executable_is_not_picked_up() {
    setup_test
    printf 'not executable\n' >"${PATH_DIR}/rlsp-yaml"
    # Deliberately no chmod +x.

    stub_uname "$PATH_DIR"
    UNAME_STUB_OS="Linux"
    UNAME_STUB_ARCH="x86_64"
    stub_curl "$PATH_DIR"
    CURL_STUB_MODE=fail
    CURL_STUB_LOG="${TEST_TMP}/curl.log"
    run

    assert_eq "$RUN_EXIT" "0" "exit code"
    assert_file_absent "${DATA_DIR}/rlsp-yaml"
    assert_invoked "$CURL_STUB_LOG" "curl"
}

# ==== Already-provisioned reuse ============================================

test_already_provisioned_binary_is_reused_noop() {
    setup_test
    mkdir -p "$DATA_DIR"
    existing_content="existing provisioned binary bytes"
    printf '%s' "$existing_content" >"${DATA_DIR}/rlsp-yaml"
    chmod 755 "${DATA_DIR}/rlsp-yaml"

    stub_curl "$PATH_DIR"
    CURL_STUB_MODE=fail
    CURL_STUB_LOG="${TEST_TMP}/curl.log"
    run

    assert_eq "$RUN_EXIT" "0" "exit code"
    assert_not_invoked "$CURL_STUB_LOG" "curl"
    assert_eq "$(cat "${DATA_DIR}/rlsp-yaml")" "$existing_content" "data-dir binary content"
}

test_stale_non_executable_data_dir_file_triggers_reprovision() {
    setup_test
    mkdir -p "$DATA_DIR"
    printf 'stale partial binary\n' >"${DATA_DIR}/rlsp-yaml"
    # Deliberately no chmod +x — simulates a prior interrupted run.

    stub_uname "$PATH_DIR"
    UNAME_STUB_OS="Linux"
    UNAME_STUB_ARCH="x86_64"
    stub_curl "$PATH_DIR"
    CURL_STUB_MODE=fail
    CURL_STUB_LOG="${TEST_TMP}/curl.log"
    run

    assert_eq "$RUN_EXIT" "0" "exit code"
    assert_invoked "$CURL_STUB_LOG" "curl"
}

# ==== Tool-availability guards ==============================================

test_missing_uname_prints_guidance_no_crash() {
    setup_test "uname"
    run
    assert_eq "$RUN_EXIT" "0" "exit code"
    assert_stdout_contains "uname"
    assert_file_absent "${DATA_DIR}/rlsp-yaml"
}

test_missing_curl_prints_guidance_before_any_platform_work_wasted() {
    setup_test "curl"
    stub_uname "$PATH_DIR"
    UNAME_STUB_OS="Linux"
    UNAME_STUB_ARCH="x86_64"
    run
    assert_eq "$RUN_EXIT" "0" "exit code"
    assert_stdout_contains "curl"
    assert_file_absent "${DATA_DIR}/rlsp-yaml"
}

test_missing_sha_tool_prints_guidance_no_download_attempted() {
    setup_test "sha256sum shasum"
    stub_uname "$PATH_DIR"
    UNAME_STUB_OS="Linux"
    UNAME_STUB_ARCH="x86_64"
    stub_curl "$PATH_DIR"
    CURL_STUB_MODE=fail
    CURL_STUB_LOG="${TEST_TMP}/curl.log"
    run
    assert_eq "$RUN_EXIT" "0" "exit code"
    assert_stdout_contains "sha256sum"
    assert_stdout_contains "shasum"
    assert_not_invoked "$CURL_STUB_LOG" "curl"
    assert_file_absent "${DATA_DIR}/rlsp-yaml"
}

# ==== Platform mapping =======================================================

# Stubs curl to log its args and fail — cheap way to observe which asset
# URL the target-mapping case statement built, without a real/fixture
# download.
assert_target_mapping() {
    os="$1"
    arch="$2"
    expected_asset="$3"
    setup_test
    stub_uname "$PATH_DIR"
    UNAME_STUB_OS="$os"
    UNAME_STUB_ARCH="$arch"
    stub_curl "$PATH_DIR"
    CURL_STUB_MODE=fail
    CURL_STUB_LOG="${TEST_TMP}/curl.log"
    run
    assert_eq "$RUN_EXIT" "0" "exit code"
    assert_invoked "$CURL_STUB_LOG" "curl"
    if ! grep -q -- "$expected_asset" "$CURL_STUB_LOG"; then
        fail "expected curl invocation to reference $expected_asset, got: $(cat "$CURL_STUB_LOG")"
    fi
}

test_target_mapping_linux_x86_64() {
    assert_target_mapping "Linux" "x86_64" "rlsp-yaml-x86_64-unknown-linux-gnu.tar.gz"
}

test_target_mapping_linux_aarch64() {
    assert_target_mapping "Linux" "aarch64" "rlsp-yaml-aarch64-unknown-linux-gnu.tar.gz"
}

test_target_mapping_linux_riscv64() {
    assert_target_mapping "Linux" "riscv64" "rlsp-yaml-riscv64gc-unknown-linux-gnu.tar.gz"
}

test_target_mapping_darwin_x86_64() {
    assert_target_mapping "Darwin" "x86_64" "rlsp-yaml-x86_64-apple-darwin.tar.gz"
}

test_target_mapping_darwin_arm64() {
    assert_target_mapping "Darwin" "arm64" "rlsp-yaml-aarch64-apple-darwin.tar.gz"
}

# ==== Unsupported platform ====================================================

assert_unsupported_platform() {
    os="$1"
    arch="$2"
    setup_test
    stub_uname "$PATH_DIR"
    UNAME_STUB_OS="$os"
    UNAME_STUB_ARCH="$arch"
    stub_curl "$PATH_DIR"
    CURL_STUB_MODE=fail
    CURL_STUB_LOG="${TEST_TMP}/curl.log"
    run
    assert_eq "$RUN_EXIT" "0" "exit code"
    assert_not_invoked "$CURL_STUB_LOG" "curl"
    assert_stdout_contains "unsupported platform"
    assert_file_absent "${DATA_DIR}/rlsp-yaml"
}

test_unsupported_arch_on_supported_os_linux_armv7l() {
    assert_unsupported_platform "Linux" "armv7l"
}

test_unsupported_arch_on_supported_os_darwin_i386() {
    assert_unsupported_platform "Darwin" "i386"
}

test_unsupported_os_entirely() {
    # A git-bash/WSL-shaped uname -s value — this script targets Linux/
    # macOS only (see the plan's Windows non-goal); if it's ever invoked
    # under something Windows-shaped, it must degrade to guidance, not crash.
    assert_unsupported_platform "MINGW64_NT-10.0-19045" "x86_64"
}

# ==== Download / integrity / extract ========================================

# Keep in sync with the Linux/x86_64 digest hardcoded in provision.sh for
# PINNED_TAG. Verified 2026-07-09 against `gh release view rlsp-yaml-v0.13.0
# --json assets` — matches GitHub's published asset digest exactly.
LINUX_X86_64_TARGET_SHA256="49a33187dcb015d2beaa11d27e2e0f2dc692e6229b075a9a590bfeb1ca8e17a9"

# Runs provision.sh for the Linux/x86_64 target against fixture tarball $2,
# with the sha256sum stub in $1 mode ("match" or "mismatch"). Sets RUN_EXIT
# etc. as usual; the caller does the assertions.
run_with_fixture() {
    sha_mode="$1"
    fixture="$2"
    stub_uname "$PATH_DIR"
    UNAME_STUB_OS="Linux"
    UNAME_STUB_ARCH="x86_64"
    stub_curl "$PATH_DIR"
    CURL_STUB_MODE=copy
    CURL_STUB_FIXTURE="$fixture"
    CURL_STUB_LOG="${TEST_TMP}/curl.log"
    stub_sha256sum "$PATH_DIR"
    SHA_STUB_MODE="$sha_mode"
    SHA_STUB_EXPECTED="$LINUX_X86_64_TARGET_SHA256"
    run
}

test_happy_path_download_verify_extract_installs_executable_binary() {
    setup_test
    fixture="${TEST_TMP}/fixture.tar.gz"
    fixture_content="fake rlsp-yaml binary bytes"
    fixture_valid_tarball "$fixture" "$fixture_content"

    run_with_fixture "match" "$fixture"

    assert_eq "$RUN_EXIT" "0" "exit code"
    assert_file_exists "${DATA_DIR}/rlsp-yaml"
    assert_executable "${DATA_DIR}/rlsp-yaml"
    assert_eq "$(cat "${DATA_DIR}/rlsp-yaml")" "$fixture_content" "installed binary content"
    assert_data_dir_contains_only_binary "$DATA_DIR"
}

test_checksum_mismatch_rejects_download_no_binary_left() {
    setup_test
    fixture="${TEST_TMP}/fixture.tar.gz"
    fixture_valid_tarball "$fixture" "fake rlsp-yaml binary bytes"

    run_with_fixture "mismatch" "$fixture"

    assert_eq "$RUN_EXIT" "0" "exit code"
    assert_stdout_contains "checksum"
    assert_data_dir_empty "$DATA_DIR"
}

# Checksum stub is in match mode here on purpose — the digest step passes
# on the garbage bytes (the stub ignores actual content), forcing execution
# into the `tar tzf`/`tar tvzf` listing step, which is the only way to
# reach that branch (a real checksum mismatch would short-circuit first).
test_unreadable_tarball_after_hash_pass_rejects_no_binary_left() {
    setup_test
    fixture="${TEST_TMP}/fixture.tar.gz"
    fixture_garbage_file "$fixture"

    run_with_fixture "match" "$fixture"

    assert_eq "$RUN_EXIT" "0" "exit code"
    assert_stdout_contains "extract"
    assert_data_dir_empty "$DATA_DIR"
}

test_multi_entry_tarball_rejected() {
    setup_test
    fixture="${TEST_TMP}/fixture.tar.gz"
    fixture_multi_entry_tarball "$fixture"

    run_with_fixture "match" "$fixture"

    assert_eq "$RUN_EXIT" "0" "exit code"
    assert_stdout_contains "unexpected contents"
    assert_data_dir_empty "$DATA_DIR"
}

test_empty_tarball_rejected() {
    setup_test
    fixture="${TEST_TMP}/fixture.tar.gz"
    fixture_empty_tarball "$fixture"

    run_with_fixture "match" "$fixture"

    assert_eq "$RUN_EXIT" "0" "exit code"
    assert_stdout_contains "unexpected contents"
    assert_data_dir_empty "$DATA_DIR"
}

# Security-relevant: proves a symlink entry literally named "rlsp-yaml" is
# rejected via the entry-type check (before extraction), not silently
# followed. Cross-check against the security-engineer's assessment.
test_symlink_entry_named_binary_rejected() {
    setup_test
    fixture="${TEST_TMP}/fixture.tar.gz"
    fixture_symlink_tarball "$fixture"

    run_with_fixture "match" "$fixture"

    assert_eq "$RUN_EXIT" "0" "exit code"
    assert_stdout_contains "unexpected contents"
    assert_data_dir_empty "$DATA_DIR"
}

test_wrong_entry_name_rejected() {
    setup_test
    fixture="${TEST_TMP}/fixture.tar.gz"
    fixture_wrong_name_tarball "$fixture"

    run_with_fixture "match" "$fixture"

    assert_eq "$RUN_EXIT" "0" "exit code"
    assert_stdout_contains "unexpected contents"
    assert_data_dir_empty "$DATA_DIR"
}

# Security-relevant: proves the exact whole-line entry-name match (not a
# whitespace-split parse) is what rejects a crafted name containing a
# space. Cross-check against the security-engineer's assessment.
test_path_traversal_style_entry_name_rejected() {
    setup_test
    fixture="${TEST_TMP}/fixture.tar.gz"
    fixture_traversal_named_tarball "$fixture"

    run_with_fixture "match" "$fixture"

    assert_eq "$RUN_EXIT" "0" "exit code"
    assert_stdout_contains "unexpected contents"
    assert_data_dir_empty "$DATA_DIR"
}

# ==== Real-network smoke test (opt-in) ======================================

# Drives an initialize -> initialized -> didOpen handshake against $1 over
# its real stdio, mirroring rlsp-yaml/tests/claude_code_stdio_smoke.rs
# (same messages, same syntax-error fixture). Unlike that Rust test, this
# doesn't parse Content-Length frames strictly — a fixed-timeout substring
# grep over the accumulated raw stdout is sufficient to prove "the binary
# answers an LSP initialize handshake over stdio" (the acceptance
# criterion) without a JSON parser in POSIX sh, and framing bytes never
# collide with the substrings we look for since Content-Length headers only
# ever precede a body, never appear inside one.
assert_binary_answers_lsp_handshake() {
    binary="$1"
    lsp_fifo="${TEST_TMP}/lsp.in"
    lsp_stdout="${TEST_TMP}/lsp.out"
    mkfifo "$lsp_fifo"
    : >"$lsp_stdout"

    "$binary" <"$lsp_fifo" >"$lsp_stdout" 2>"${TEST_TMP}/lsp.err" &
    server_pid=$!
    exec 3>"$lsp_fifo"

    send_lsp_message '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{},"processId":null,"rootUri":null}}'
    send_lsp_message '{"jsonrpc":"2.0","method":"initialized","params":{}}'
    send_lsp_message '{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///smoke/bad.yaml","languageId":"yaml","version":1,"text":"key: [bad\n"}}}'

    got_response=0
    got_diagnostics=0
    deadline=$(($(date +%s) + 5))
    while [ "$(date +%s)" -lt "$deadline" ]; do
        if [ "$got_response" -eq 0 ] && grep -q '"id":1' "$lsp_stdout" 2>/dev/null; then
            got_response=1
        fi
        if grep -q 'textDocument/publishDiagnostics' "$lsp_stdout" 2>/dev/null; then
            got_diagnostics=1
            break
        fi
        sleep 0.1
    done

    exec 3>&-
    kill "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true

    if [ "$got_response" -eq 0 ]; then
        fail "expected an initialize response (id 1) from $binary within 5s, got: $(cat "$lsp_stdout")"
    fi
    if [ "$got_diagnostics" -eq 0 ]; then
        fail "expected a textDocument/publishDiagnostics notification from $binary within 5s, got: $(cat "$lsp_stdout")"
    fi
}

# Writes one Content-Length-framed JSON-RPC message ($1, the body) to the
# handshake fifo opened on fd 3 by assert_binary_answers_lsp_handshake.
send_lsp_message() {
    body="$1"
    len=$(printf '%s' "$body" | wc -c | tr -d ' ')
    printf 'Content-Length: %s\r\n\r\n%s' "$len" "$body" >&3
}

# Unstubbed curl/uname/sha256sum — runs the real script against the real
# pinned release. Assumes the host maps to one of the published Linux/macOS
# targets (true for this sandbox); skipped by default, see the file header.
test_real_release_download_end_to_end() {
    setup_test
    run
    assert_eq "$RUN_EXIT" "0" "exit code"
    assert_file_exists "${DATA_DIR}/rlsp-yaml"
    assert_executable "${DATA_DIR}/rlsp-yaml"
    assert_binary_answers_lsp_handshake "${DATA_DIR}/rlsp-yaml"
}

# ==== Runner =================================================================

run_all_tests() {
    total=0
    failed=0
    for name in $(grep -o '^test_[a-zA-Z0-9_]*' "$0"); do
        if [ "$name" = "test_real_release_download_end_to_end" ] && [ "${PROVISION_TEST_NETWORK:-0}" != "1" ]; then
            printf 'SKIP (network): %s\n' "$name"
            continue
        fi
        total=$((total + 1))
        CURRENT_TEST="$name"
        if ( "$name" ); then
            printf 'PASS: %s\n' "$name"
        else
            failed=$((failed + 1))
            printf 'FAIL: %s\n' "$name" >&2
        fi
    done
    printf '\n%d/%d passed\n' "$((total - failed))" "$total"
    [ "$failed" -eq 0 ]
}

run_all_tests
