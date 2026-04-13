**Repository:** root
**Status:** InProgress
**Created:** 2026-04-13

## Goal

Add missing integration tests for the base `yaml11Boolean`
and `yaml11Octal` diagnostics through the full LSP pipeline
(`did_change` → `publish_diagnostics`). The current tests
cover code actions (fabricating diagnostics) and schema-aware
variants, but no test verifies that opening a document with
`enabled: yes` or `mode: 0777` actually emits the base
diagnostics through the server handler.

## Context

- The YAML 1.1 compatibility diagnostics were implemented
  in plan `2026-04-13-yaml-11-compatibility-diagnostics.md`
  (completed)
- Task 1 added `validate_yaml11_compat()` in `validators.rs`
  with unit tests, but no integration test through the
  server pipeline
- Task 2 added code action integration test
  (`should_return_yaml11_bool_code_actions_via_server_handler`)
  but it fabricates the diagnostic — it doesn't verify the
  server emits it
- Task 3 added proper LSP pipeline integration tests for
  schema-aware variants (`schemaYaml11Boolean`, etc.) using
  `get_diagnostics()` — this is the pattern to follow
- Integration tests live in `rlsp-yaml/tests/lsp_lifecycle.rs`
- Pattern: `LspService::new` → `initialize` → `did_open` →
  `get_diagnostics()` → assert on diagnostic codes/severity

### Key files

- `rlsp-yaml/tests/lsp_lifecycle.rs` — integration tests
- `rlsp-yaml/src/validation/validators.rs` —
  `validate_yaml11_compat()` implementation
- `rlsp-yaml/src/server.rs` — diagnostic pipeline wiring

## Steps

- [x] Add integration test: `yaml11Boolean` diagnostic
  emitted for plain `yes`/`no`/`on`/`off` values
- [x] Add integration test: `yaml11Octal` diagnostic
  emitted for plain `0777` values
- [x] Add integration test: both diagnostics suppressed
  when `$yamlVersion=1.1` modeline present
- [x] Verify no diagnostic on quoted values (`"yes"`,
  `"0777"`)

## Tasks

### Task 1: Add yaml11Boolean/yaml11Octal integration tests

Add integration tests to `rlsp-yaml/tests/lsp_lifecycle.rs`
following the pattern used by the schema-aware tests
(lines 2564-2684). No schema needed — just open a document
and verify diagnostics.

Three tests:

1. **`should_emit_yaml11_boolean_warning_for_plain_scalars`**
   — open `enabled: yes\nactive: on\n`, verify
   `yaml11Boolean` warnings emitted, severity is WARNING,
   verify quoted `name: "yes"\n` does NOT produce the
   diagnostic

2. **`should_emit_yaml11_octal_info_for_plain_scalars`**
   — open `mode: 0777\n`, verify `yaml11Octal` diagnostic
   emitted, severity is INFORMATION, verify quoted
   `mode: "0644"\n` does NOT produce the diagnostic

3. **`should_suppress_yaml11_compat_diagnostics_in_v1_1_mode`**
   — open with `$yamlVersion=1.1` modeline + `enabled: yes\nmode: 0777\n`, verify zero
   `yaml11Boolean` / `yaml11Octal` diagnostics

- [x] Test 1: yaml11Boolean emission (5d22db0)
- [x] Test 2: yaml11Octal emission (5d22db0)
- [x] Test 3: yamlVersion=1.1 suppression (5d22db0)
- [x] `cargo test` passes
- [x] `cargo clippy --all-targets` zero warnings

## Decisions

- **Single task** — all three tests are small, follow the
  same pattern, and belong in the same file section. No
  benefit to splitting.
- **No advisor needed** — pattern-following with existing
  test coverage as template.
