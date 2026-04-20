**Repository:** root
**Status:** Completed (2026-04-04)
**Created:** 2026-04-04

## Goal

Add YAML version selection (1.1 vs 1.2) as a workspace setting and
per-document modeline. YAML 1.1 treats `on`/`off`/`yes`/`no` as booleans
and has different octal number syntax — Ansible assumes 1.1, Kubernetes and
GitHub Actions assume 1.2. Without this setting, users in mixed-version repos
can't get correct output for all files.

**Important limitation:** saphyr (our YAML parser) is YAML 1.2 only. It only
recognizes `true`/`false` as booleans — `on`/`off`/`yes`/`no` are always
parsed as strings. There is no 1.1 parsing mode. This setting therefore
affects **formatter output and diagnostics only**, not parsing. Specifically:
- **Formatter:** controls which values `needs_quoting()` considers reserved
  (1.1 mode adds `on`/`off`/`yes`/`no` to the quoting list so output is
  safe for 1.1 consumers like Ansible)
- **Diagnostics:** may adjust version-sensitive warnings
- **Parser:** unchanged — always resolves values per YAML 1.2 core schema

This is sufficient for the primary use case (producing output safe for
1.1-consuming tools) but does NOT provide true 1.1 value resolution (where
`on:` is parsed as `true:`). True 1.1 support would require a different
parser.

## Context

- saphyr is YAML 1.2 only — `scalar.rs` resolves only `true`/`false` as
  booleans; `on`/`off`/`yes`/`no` become strings
- The saphyr emitter does quote 1.1 keywords for interop safety (line 455)
- The `needs_quoting` function in `formatter.rs` lists YAML 1.1 boolean
  keywords (`on`, `off`, `yes`, `no`, etc.) — these need quoting in 1.1
  but not in 1.2
- Modeline parsing exists in `schema.rs` for `$schema=` and `$tags=`,
  scanning first 10 lines with prefix `# yaml-language-server:`
- Settings struct in `server.rs` (line 33) is deserialized from workspace
  config
- RedHat's yaml-language-server has `yaml.yamlVersion` setting

### YAML 1.1 vs 1.2 differences affecting our implementation

1. **Boolean keywords:** 1.1 treats `yes`, `no`, `on`, `off`, `y`, `n`
   (and capitalized variants) as booleans. 1.2 only recognizes `true`/`false`.
2. **Octal numbers:** 1.1 uses `0` prefix (e.g., `0644`), 1.2 uses `0o`
   prefix (e.g., `0o644`). `0644` in 1.2 is a string, not octal.
3. **Sexagesimal numbers:** 1.1 supports `1:30:00` as 5400 (base-60). 1.2
   treats this as a string.

### Key files

- `rlsp-yaml/src/server.rs` — Settings struct, `parse_and_publish()`
- `rlsp-yaml/src/formatter.rs` — `needs_quoting()`, scalar handling
- `rlsp-yaml/src/schema.rs` — modeline parsing functions
- `rlsp-yaml/src/validators.rs` — validators may need version-aware behavior
- `rlsp-yaml/docs/configuration.md` — settings documentation

## Steps

- [x] Add `yamlVersion` setting (67e1401)
- [x] Add `$yamlVersion` modeline support (67e1401)
- [x] Adjust `needs_quoting` for version (b514adc)
- [x] Adjust validators for version (c58fd56 — no changes needed, confirmed version-agnostic)
- [x] Add tests (c58fd56)
- [x] Update documentation (9fdd6fb)

## Tasks

### Task 1: Add yamlVersion setting and modeline (67e1401)

Add `yaml_version` to the Settings struct and a modeline parser for
`$yamlVersion=1.1` or `$yamlVersion=1.2`.

- [x] Add `yaml_version: Option<String>` to Settings (default: None → 1.2)
- [x] Add `extract_yaml_version(text)` in `schema.rs` following existing
      modeline pattern
- [x] Modeline overrides workspace setting (same priority as `$schema`)
- [x] Validate version is "1.1" or "1.2" — ignore invalid values
- [x] Unit tests for modeline parsing
- [x] Unit tests for setting deserialization

### Task 2: Version-aware quoting in formatter (b514adc)

Adjust `needs_quoting()` to accept a YAML version parameter. In 1.2 mode,
`on`/`off`/`yes`/`no` don't need quoting (they're plain strings). In 1.1
mode, they do.

- [x] Add version parameter to `needs_quoting()`
- [x] Split the reserved-words list: always-reserved (1.1+1.2) vs 1.1-only
- [x] Thread version through `format_yaml()` → `string_to_doc()` →
      `needs_quoting()`
- [x] Plumb version from Settings/modeline to formatter call in server.rs
- [x] Tests: `on:` not quoted in 1.2, quoted in 1.1
- [x] Tests: `true`/`false`/`null` always quoted (both versions)

### Task 3: Version-aware diagnostics (c58fd56)

Adjust any version-sensitive diagnostic behavior.

- [x] Evaluate if any validators need version awareness (e.g., duplicate
      key semantics are the same in both versions) — no changes needed
- [x] If octal/sexagesimal values affect schema validation, adjust — N/A, saphyr parses 0644 as String
- [x] Tests for version-specific diagnostic behavior — confirmation test added

### Task 4: Update documentation (9fdd6fb)

- [x] Add `yamlVersion` to `docs/configuration.md`
- [x] Add `$yamlVersion` modeline syntax
- [x] Document 1.1 vs 1.2 behavioral differences

## Decisions

- **Default to 1.2** — YAML 1.2 is the current standard and saphyr's default.
  Most modern tools (K8s, GHA) use 1.2. Ansible users can opt into 1.1.
- **Modeline overrides setting** — follows the same priority as `$schema`.
  A mixed repo can use 1.2 globally with `$yamlVersion=1.1` in Ansible files.
- **Formatter and diagnostics only — not the parser** — saphyr is YAML 1.2
  only with no 1.1 mode. This setting controls quoting decisions and
  diagnostic behavior, not value resolution. True 1.1 parsing (where `on:`
  resolves to `true:`) would require replacing the parser.
