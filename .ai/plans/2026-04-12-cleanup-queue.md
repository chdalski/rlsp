**Repository:** root
**Status:** InProgress
**Created:** 2026-04-12

# Cleanup queue — validated items from code-improvements plan

## Goal

Apply four validated cleanup items (C1, C2 partial, C3, C4a) accumulated during the 2026-04-11 code-improvements plan. Each item was investigated against the current codebase on 2026-04-12 and confirmed valid. Items found invalid during investigation (C2 color.rs/line_mapping.rs/scalar_helpers.rs/server.rs, C4b, C4c, C4d) are excluded.

## Context

- The cleanup queue lives in `.ai/memory/project_followup_plans.md` under "Open — Cleanup queue"
- All items are pure refactoring — no behavioral changes, no new features
- The code-improvements plan that surfaced these items is already Completed
- Workspace lints enforce `#[expect(reason)]` over `#[allow]`, clippy pedantic+nursery at warn, `warnings = "deny"`

### Items included

| ID | Description | Crate(s) | Risk |
|----|-------------|----------|------|
| C1 | Remove 3 stale `file.rs:NNN` references in test comments | rlsp-yaml-parser | Trivial |
| C2 | Convert `parse_block_header` if/else chain to match arms | rlsp-yaml-parser | Low |
| C4a | Replace 5 `for _ in 0..n { push('\n') }` loops with `repeat_n` | rlsp-yaml-parser | Trivial |
| C3 | Extract `PlainScalarKind` enum + `classify_plain_scalar` helper | rlsp-yaml | Low-moderate |

### Items excluded (with reasons)

- **C2 — color.rs**: `starts_with()` calls cannot be pattern-matched
- **C2 — line_mapping.rs**: bitmask ops need match guards, no clarity gain
- **C2 — scalar_helpers.rs**: needs to know which prefix matched for radix
- **C2 — server.rs**: heterogeneous return types make match impossible
- **C4b**: `str::replace` chain is semantically incorrect — mishandles `\r\n\r`
- **C4c**: `drain_to_end` has per-iteration side effects incompatible with `.last()`
- **C4d**: second branch has complex inline logic that doesn't fit `matches!`

## Steps

- [x] Fix stale line-number references in test comments (C1) — 10be323
- [x] Replace `for _ in 0..n` newline loops with `repeat_n` (C4a) — 10be323
- [x] Convert `parse_block_header` if/else to match (C2) — 10be323
- [x] Extract `PlainScalarKind` enum and `classify_plain_scalar` helper (C3) — 6569e1c
- [x] Grep for any remaining `\w+\.rs:\d+` references (verification) — 10be323
- [x] Run `cargo test`, `cargo clippy --all-targets`, `cargo fmt --check` — 6569e1c

## Tasks

### Task 1: Parser-crate cleanups (C1 + C4a + C2)

All changes are in `rlsp-yaml-parser`. No behavioral change — pure refactoring.

**C1 — Rewrite 3 stale line-number references:**

- `rlsp-yaml-parser/tests/robustness.rs:64` — comment says `loader.rs:399` for anchor registration. Rewrite: describe that the loader registers mapping/sequence anchors after parsing their content (in the node-expansion path), not at the `&anchor` token. Do not reference a line number.
- `rlsp-yaml-parser/tests/robustness.rs:98` — comment says `loader.rs:528` for CircularAlias code path. Rewrite: name the function that contains the CircularAlias defense (grep for `CircularAlias` in `loader.rs` to find it) and describe what it guards against. Do not reference a line number.
- `rlsp-yaml-parser/tests/smoke/tags.rs:508` — comment says `consume_line` at `lib.rs:1124`. Rewrite: describe the "unrecognised content" fallback path by its current function name (grep for `consume_line` or the relevant fallback). Do not reference a line number.

**Verification:** after fixing C1, grep for `\w+\.rs:\d+` across `rlsp-yaml-parser/` to confirm no new hits.

**C4a — Replace 5 newline-push loops with `repeat_n`:**

- `rlsp-yaml-parser/src/lexer/quoted.rs` (~line 332) — `for _ in 0..pending_blanks { owned.push('\n'); }` → `owned.extend(std::iter::repeat_n('\n', pending_blanks));`
- `rlsp-yaml-parser/src/lexer/block.rs` (~line 203) — `for _ in 0..trailing_newlines { out.push('\n'); }` → same pattern
- `rlsp-yaml-parser/src/lexer/block.rs` (~line 384) — second `trailing_newlines` flush → same pattern
- `rlsp-yaml-parser/src/lexer/block.rs` (~line 621) — `trailing_blank_count` flush → same pattern
- `rlsp-yaml-parser/src/lexer/plain.rs` (~line 225) — `for _ in 0..pending_blanks { buf.push('\n'); }` → same pattern

Check whether `repeat_n` needs a `use` import or is available via `std::iter::repeat_n` inline. The variable types (`usize`) are compatible.

**C2 — Convert `parse_block_header` if/else to match:**

In `rlsp-yaml-parser/src/lexer/block.rs`, function `parse_block_header`, the `Some(ch) =>` arm (~line 496) contains an if/else-if chain testing `ch == '+'`, `ch == '-'`, `ch.is_ascii_digit()`. Convert to match arms:

```rust
Some('+') => { /* chomp Keep logic */ }
Some('-') => { /* chomp Strip logic */ }
Some('0') => { /* error: indent 0 invalid */ }
Some(ch @ '1'..='9') => { /* indent indicator logic */ }
Some(_) => { /* error: invalid indicator */ }
```

This eliminates the nested `ch == '0'` check inside the `is_ascii_digit()` branch. The `chomp.is_some()` duplicate-chomp checks remain in each arm — do not extract them (each arm returns a different error position).

- [x] Rewrite 3 stale line-number comments (C1)
- [x] Replace 5 for-loops with `repeat_n` (C4a)
- [x] Verify no remaining `\w+\.rs:\d+` patterns in parser crate
- [x] Refactor `parse_block_header` `Some(ch) =>` arm to direct pattern matching (C2)
- [x] `cargo test`, `cargo clippy --all-targets`, `cargo fmt --check`

### Task 2: Extract `PlainScalarKind` enum and `classify_plain_scalar`

**New type in `rlsp-yaml/src/scalar_helpers.rs`:**

```rust
pub enum PlainScalarKind {
    Null,
    Bool,
    Integer,
    Float,
    String,
}

pub fn classify_plain_scalar(value: &str) -> PlainScalarKind {
    if is_null(value) {
        PlainScalarKind::Null
    } else if is_bool(value) {
        PlainScalarKind::Bool
    } else if is_integer(value) {
        PlainScalarKind::Integer
    } else if is_float(value) {
        PlainScalarKind::Float
    } else {
        PlainScalarKind::String
    }
}
```

**Refactor `yaml_type_name` in `rlsp-yaml/src/schema_validation/mod.rs` (~line 1402):**

Replace the is_null/is_bool/is_integer/is_float chain with:
```rust
match scalar_helpers::classify_plain_scalar(value) {
    PlainScalarKind::Null => "null",
    PlainScalarKind::Bool => "boolean",
    PlainScalarKind::Integer => "integer",
    PlainScalarKind::Float => "number",
    PlainScalarKind::String => "string",
}
```

Keep the `ScalarStyle::Plain` early-return as-is — it guards the classification.

**Refactor `node_symbol_kind` in `rlsp-yaml/src/symbols.rs` (~line 379):**

Replace the chain with:
```rust
match scalar_helpers::classify_plain_scalar(value) {
    PlainScalarKind::Null => SymbolKind::NULL,
    PlainScalarKind::Bool => SymbolKind::BOOLEAN,
    PlainScalarKind::Integer | PlainScalarKind::Float => SymbolKind::NUMBER,
    PlainScalarKind::String => SymbolKind::STRING,
}
```

**Preserve divergences exactly:**
- `yaml_type_name`: style early-return stays, `Integer` → `"integer"`, `Float` → `"number"`, `Alias` → `"unknown"`
- `node_symbol_kind`: no style check, `Integer | Float` → `NUMBER`, `Alias` → `STRING`
- The `Node::Alias` mappings are NOT changed — that divergence is a behavioral question out of scope

- [x] Add `PlainScalarKind` enum and `classify_plain_scalar` to `scalar_helpers.rs` — 6569e1c
- [x] Refactor `yaml_type_name` to use `classify_plain_scalar` — 6569e1c
- [x] Refactor `node_symbol_kind` to use `classify_plain_scalar` — 6569e1c
- [x] `cargo test`, `cargo clippy --all-targets`, `cargo fmt --check` — 6569e1c

## Decisions

- **Merged C1+C4a+C2 into one task** — all three are in `rlsp-yaml-parser`, all are mechanical refactoring with no behavioral change. One commit keeps the pipeline efficient.
- **Excluded C2 items except `parse_block_header`** — investigation showed the other four are not match-convertible or already clear
- **Excluded C4b/C4c/C4d** — C4b is semantically incorrect, C4c has side-effect incompatibility, C4d's second branch is too complex for `matches!`
- **Task ordering: parser-crate first, then cross-crate** — no dependencies, but groups by blast radius
- **No advisor consultation needed** — all tasks are pure refactoring with no behavioral changes, no new API surface, no trust boundaries. Existing tests cover all modified code paths
