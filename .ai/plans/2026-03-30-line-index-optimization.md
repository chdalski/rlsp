**Repository:** root
**Status:** NotStarted
**Created:** 2026-03-30

## Goal

Eliminate O(n²) behavior in `validate_key_ordering` and
`validate_schema` by replacing per-diagnostic linear scans
with a pre-built line index. Benchmarks show these two
functions account for 97% of per-keystroke cost on large
files due to `find_key_line` / `find_key_range` scanning
from line 0 for every diagnostic emitted.

## Context

- Benchmark analysis shows `validate_key_ordering` takes
  6.38 ms on 2K-line YAML (95.5% of total validator cost)
  and `schema_validation` takes 162 ms on 10K-line YAML —
  both exhibiting O(n²) scaling.
- Root cause: `find_key_line` (validators.rs:546) and
  `find_key_range` (schema_validation.rs:1833) scan ALL
  lines from the top for each diagnostic. When many
  diagnostics are emitted (common with key-ordering
  violations or schema mismatches), this becomes O(k×n)
  where k = diagnostics and n = lines.
- Fix: build a `HashMap<String, u32>` (key → first line
  number) once at the start of each function, then do O(1)
  lookups. Total cost becomes O(n) for index building +
  O(k) for lookups = O(n+k).
- This is a pure refactoring — same matching logic, same
  outputs, same diagnostic positions. No behavior change.
  All existing tests must continue to pass unchanged.
- The two files have slightly different key-matching logic:
  - validators.rs: `trimmed.starts_with(key) &&
    trimmed[key.len()..].trim_start().starts_with(':')`
  - schema_validation.rs: also handles `- key:` prefix
    stripping and `key ` (space) matching
  Each file builds its own index with its own matching
  logic — sharing would add coupling for minimal benefit.

### Key files

- `rlsp-yaml/src/validators.rs` — `find_key_line` at
  line 546, called from `check_yaml_ordering`
- `rlsp-yaml/src/schema_validation.rs` — `find_key_range`
  at line 1833, called from `node_range`, `mapping_range`,
  `key_range`; `Ctx` struct at line 157

## Steps

- [x] Clarify requirements with user
- [ ] Add line index to validate_key_ordering
- [ ] Add line index to validate_schema

## Tasks

### Task 1: Add line index to validate_key_ordering

Replace `find_key_line` linear scan with index lookup in
`validators.rs`.

- [ ] Build a `HashMap<String, u32>` at the top of
      `validate_key_ordering` by scanning `lines` once,
      extracting the key portion of each line using the same
      matching logic as current `find_key_line` (trim start,
      check for `key:` pattern), first-match-wins insertion
- [ ] Pass the index to `check_yaml_ordering` as a new
      parameter
- [ ] Replace `find_key_line(key, lines)` call inside
      `check_yaml_ordering` with `index.get(key).copied()`
- [ ] Remove `find_key_line` function (dead code after
      refactoring)
- [ ] Verify `cargo test` passes (no behavior change)
- [ ] Run `cargo bench --bench insight` to measure
      improvement

### Task 2: Add line index to validate_schema

Replace `find_key_range` linear scan with index lookup in
`schema_validation.rs`.

- [ ] Add a `key_index: HashMap<String, u32>` field to
      `Ctx` struct
- [ ] Build the index at the top of `validate_schema` using
      the same matching logic as current `find_key_range`
      (handle `- ` prefix stripping, match `key:` or
      `key ` patterns, strip `[0]` brackets), first-match-wins
- [ ] Pass the index through `Ctx::new` (add parameter)
- [ ] Replace `find_key_range` body: look up key in
      `ctx.key_index` / the index, return the stored range
      on hit, fall back to `(0,0)-(0,0)` on miss
- [ ] The `find_key_range` callers (`node_range`,
      `mapping_range`, `key_range`) already pass `lines` —
      update them to also pass the index (or access it via
      `ctx` where available)
- [ ] Remove the per-call line scanning from `find_key_range`
- [ ] Verify `cargo test` passes (no behavior change)
- [ ] Run `cargo bench --bench hot_path --bench insight` to
      measure improvement

## Decisions

- **Separate indexes per file** — validators.rs and
  schema_validation.rs have different key-matching logic.
  Sharing a utility would require parameterizing the
  matching, adding coupling for ~10 lines of savings.
  Each file builds its own index.
- **HashMap<String, u32> with first-match-wins** — matches
  current `find_key_line` / `find_key_range` behavior of
  returning the first matching line. No behavior change.
- **Store line number, not Range** — for validators.rs the
  index stores `u32` (line number) since that's what
  `find_key_line` returns. For schema_validation.rs, the
  index stores `u32` and `find_key_range` computes the
  Range from the stored line + key length (same as current
  code, just without re-scanning).
