---
name: Code quality audit — 2026-04-11
description: Combined findings from three parallel research passes (readability/structure, performance/idioms, docs/consistency)
type: project
---

# Code Quality Audit — 2026-04-11

Snapshot combining three parallel research passes over `rlsp-fmt/`, `rlsp-yaml/`, and `rlsp-yaml-parser/`. Findings are deduplicated and grouped by theme rather than by auditor. Severity reflects each auditor's judgment; where two auditors flagged the same symbol from different angles, the findings are merged.

**Why this exists:** the user asked for a broad code-quality sweep. This file is the persistent record so future sessions can pick findings from it without re-running the audit. Once findings graduate into actionable plans they should move to `project_followup_plans.md` or a dedicated plan file.

## Verification pass — 2026-04-11

After the initial three-agent sweep, every high-priority finding was checked against the source. Summary of what changed:

- **H2 removed** — false positive (see H2 section for details).
- **H6 removed** — false positive. The seven `panic!` calls in `reloc.rs` are all inside `#[cfg(test)] mod tests` where `clippy::panic` is explicitly allowed; the production `reloc` function (lines 7-68) is a pure idiomatic `match` with zero panics. The agent grepped for `panic!` without checking the `#[cfg(test)]` boundary.
- **H1 corrected** — settings-lock count is 6, not 7. The agent named `format_validation` and `yaml_version` but neither is called directly from `parse_and_publish` (they're called by its helpers). Function length (117 lines) and double-clone of `diagnostics` are verified. The refactor direction is unchanged.
- **H3 counts corrected both ways:**
  - `find_mapping_colon` — **undercounted**. Verbatim copy exists in **6** files, not 2: `completion.rs`, `hover.rs`, `folding.rs`, `symbols.rs`, `semantic_tokens.rs`, `on_type_formatting.rs`. Strengthens the finding.
  - `MAX_BRANCH_COUNT` — **overcounted**. Defined in only 2 files (`completion.rs`, `schema_validation.rs`), not 3. `hover.rs` only defines `MAX_DESCRIPTION_LEN`.
  - Probe-ctx pattern — **undercounted**. The `let mut scratch = Vec::new(); let mut probe = Ctx::new(...);` pattern occurs **5 times** across `schema_validation.rs` (lines 598, 1729, 1757, 1793, 1814), not 2. Strengthens the finding.
  - `document_range` vs `document_index_for_line` — **weak finding**. They produce different outputs (line range vs doc index), use different separator matchers (`is_document_separator` vs literal `"---"`), and scan in different directions. The dedup is behavioural, not mechanical. Additionally, `references.rs` and `rename.rs` each have their own `document_range_for_line` — so there are four variants, not two. Re-scoped as "worth consolidating" but needs design, not just refactoring.
- **H4 simplified** — the suggested fix for `pos_after_line` is inferior to what already exists. `rlsp-yaml-parser/src/lines.rs:376` already has a private `const fn pos_after_line` that is O(1) and produces an identical `Pos` to the walking version in `lexer.rs`. The fix is to promote the `lines.rs` version to `pub` and delete the `lexer.rs` walker — no need to reach for `column_at` or split on ASCII.
- **H7 partially wrong** — `rlsp-yaml-parser/README.md` **does exist**. Only `docs/configuration.md` and `docs/feature-log.md` are missing (the `docs/` directory contains only `benchmarks.md`). The crate-level `//!` gap in `src/lib.rs` is confirmed.
- **Stale task comments** — the findings are a mix of genuinely stale references (post-completion residue) and **active known-limitation notes** that the auditor misclassified. See the corrected "Stale doc comments" entry below.
- **Medium finding: `formatter.rs::node_to_doc` "unnecessary clone"** — likely a false positive. `text(value.clone())` clones because `value: &String` is borrowed from the AST and `text()` consumes a `String`; removing the clone would require the AST to be passed by value. Not a win at the ownership model this codebase uses. Removed.
- **Low finding: `Settings` with 12 `Option<bool>` fields** — could not reproduce. `grep Option<bool>` in `rlsp-yaml/src/schema.rs` returns zero matches. The audit cited the wrong file, the wrong type, or the wrong count. Removed.
- **Medium finding: `validate_string_constraints` triple boilerplate** — understated. The `let range = node_range(path, ctx.key_index); ctx.diagnostics.push(make_diagnostic(...))` pattern repeats 5+ times in this function across pattern/min_length/max_length branches, not 3. Stronger finding than reported.

Everything else (H5, loader `Vec::new()`, `tag_directives` clone, `get_regex` cache, `SchemaError` manual Display, `JsonSchema` `exclusive_minimum` pairs, `completion.rs` `collect+join`) verified as reported.

## Clean areas (do not touch)

- **`rlsp-fmt`** — all three audits agree. Three files, under 200 lines each, all abstractions in active use, doc comments solid, no hot-path allocation concerns. Not a refactor target.
- **Lint inheritance** — all three crates inherit `[workspace.lints]` via `lints.workspace = true`. No drift.
- **Module organisation** — no `mod.rs` in any `src/` directory. Convention respected.
- **Test naming** — consistent `snake_case` behaviour descriptions across crates.
- **`rlsp-yaml` cross-doc consistency** — README, `docs/configuration.md`, and `docs/feature-log.md` agree; README explicitly tiers to the configuration doc, so the brevity gap is intentional.
- **`rlsp-yaml-parser` structural decomposition** — state machine is correctly factored; the two `#[allow(clippy::too_many_lines)]` on `handle_flow_collection` and `consume_mapping_entry` are justified by in-file design notes.
- **`rlsp-yaml-parser` public types** — `Event`, `Node`, `Document`, `Loader`, `LoaderBuilder`, limits constants all have good `///` docs with invariant and security notes.

## High-priority findings

### H1 — `rlsp-yaml/src/server.rs → parse_and_publish` is the LSP hot path and has stacking issues

This function is reached on every keystroke in an open document. Three issues confirmed:

- **Structure:** 117 lines blending validators, custom-tag merging, a four-fallback schema resolution chain, suppression filtering, and diagnostic publishing. Verified.
- **Settings lock contention:** `Settings` sits behind `Mutex<Settings>` and is locked up to **6 times per call** (`get_validate`, `get_key_ordering`, `get_custom_tags`, `get_schema_associations`, `get_kubernetes_version`, `get_schema_store_enabled`). Each is a short-lived acquisition and not all fire on every call path. The audit originally said "seven" and named `format_validation`/`yaml_version`, but those aren't called directly from `parse_and_publish`.
- **Diagnostic double-clone:** verified at `server.rs:368` (`result.diagnostics.clone()` to start the local mut `Vec`) and `server.rs:469` (`diagnostics.clone()` into the diagnostics store). `publish_diagnostics` then takes ownership by move at line 473.

**Combined refactor direction:** extract a `resolve_schema_for_document` helper to collapse the fallback chain; switch `settings` to `RwLock<Settings>` and read a snapshot struct at the top of `parse_and_publish`; drop one of the two diagnostic clones by restructuring the store insert to move-then-clone-once-for-publish. Together these make `parse_and_publish` both shorter and cheaper in the hot path.

### H2 — Removed 2026-04-11

Original H2 claimed `hover.rs::yaml_type_name` was a third call site for the C2/C3 scalar-classification refactor. Verification against the source showed: (a) `hover.rs::yaml_type_name` has no `is_null`/`is_bool`/`is_integer`/`is_float` chain — it only matches on the `Node` variant; and (b) the three similarly-named functions return different types because each is pinned to a different external spec — hover UI needs YAML-native vocabulary ("mapping"/"sequence"), schema_validation needs JSON Schema type strings ("object"/"array"), and symbols needs the LSP `SymbolKind` enum. The output-format divergence is required by the consumers' wire formats, not an accident of parallel evolution. Not a finding.

### H3 — Cross-module duplication inside `rlsp-yaml`

Verified findings, with corrected scale:

- **`find_mapping_colon`** — verbatim copy in **6 files**: `completion.rs`, `hover.rs`, `folding.rs`, `symbols.rs`, `semantic_tokens.rs`, `on_type_formatting.rs`. Same body: quote-aware colon scanner with `in_single_quote`/`in_double_quote` flags, returning the first colon followed by whitespace or end-of-line. Move to a shared `line_utils` helper and import from all six call sites. This is the strongest dedup opportunity in `rlsp-yaml`.
- **`MAX_DESCRIPTION_LEN` (200)** — defined 3 times in `completion.rs`, `schema_validation.rs`, `hover.rs`.
- **`MAX_BRANCH_COUNT` (20)** — defined 2 times in `completion.rs` and `schema_validation.rs`. Not in `hover.rs` (audit was wrong about `hover.rs`). Still worth extracting to a `schema_limits` module alongside `MAX_DESCRIPTION_LEN`.
- **Probe-ctx pattern** — the `let mut scratch = Vec::new(); let mut probe = Ctx::new(&mut scratch, format_validation, key_index);` dry-run pattern occurs **5 times** in `schema_validation.rs` (around lines 598, 1729, 1757, 1793, 1814), not 2. Extract `branch_passes(node, branch, path, depth, ctx_meta) -> bool` and reuse across the composition validators (`anyOf`, `oneOf`, `not`, `if/then/else`).
- **`document_range` / `document_index_for_line` / `document_range_for_line` — weak finding, 4 variants total.** `completion.rs::document_range` returns `(start, end)` using `is_document_separator` (both `---` and `...`). `hover.rs::document_index_for_line` returns a separator count using only literal `"---"`. `references.rs::document_range_for_line` and `rename.rs::document_range_for_line` each have their own variant. Unifying these would require a design decision on separator semantics, not mechanical extraction. Re-scoped from mechanical dedup to "design cleanup — worth doing but not a pure refactor."

### H4 — Parser hot-path allocations

Called on every keystroke parse:

- **`lexer.rs → pos_after_line`** — walks each character in `line.content` with `pos.advance(ch)` to produce a `Pos`. Called from ~20 sites across `lexer/comment.rs`, `lexer/block.rs`, `lexer/quoted.rs`, `lexer/plain.rs`, `lexer.rs` itself. **An O(1) implementation already exists** in `lines.rs:376` as a private `const fn pos_after_line` that computes the same `Pos` directly from `line.offset + line.content.len() + line.break_type.byte_len()` with `line += 1` and `column = 0`. Since `line.content` never contains a newline, the walking version produces the identical result. **Fix:** promote the `lines.rs` version to `pub(crate)`, re-point all imports, delete the `lexer.rs` walker. Simpler than the original audit's `column_at` fix suggestion.
- **`loader.rs → load_node`** — `entries: Vec<(Node<Span>, Node<Span>)> = Vec::new()` at line 392, `items: Vec<Node<Span>> = Vec::new()` at line 464. Both have no capacity hint. The same file uses `Vec::with_capacity(entries.len())` at lines 629/652 for the expansion path, so the pattern is already understood locally. Pre-sizing the load-path Vecs would need a cheap peek-ahead count from the event stream — probably worth it on documents with large collections, borderline on typical YAML. **Medium severity.**

### H5 — `rlsp-yaml/src/schema_validation.rs → validate_schema` collects `text.lines()` on every call

`text.lines().collect::<Vec<_>>()` runs on every validation (every keystroke), solely to feed `build_key_index`. Change `build_key_index` to accept `impl Iterator<Item = (usize, &str)>` and pass `text.lines().enumerate()` directly.

### H6 — Removed 2026-04-11 (false positive)

Original H6 claimed five `panic!` calls in `reloc.rs` violated the no-panics rule. Verification against the source showed: the production `reloc` function (`reloc.rs:7-68`) is a pure `match` over `Node` variants with zero panics — it's textbook idiomatic Rust. The panic calls the auditor found (actually seven, not five) all live inside `#[cfg(test)] mod tests` starting at line 78, inside standard test-assertion patterns (`_ => panic!("expected Scalar")` inside match arms on the function result). The test module has `#[allow(clippy::panic)]` explicitly. This is normal Rust test scaffolding, not a rule violation. The agent grepped for `panic!` without checking the `#[cfg(test)]` module boundary.

### H7 — `rlsp-yaml-parser` partial documentation gap

- **No crate-level `//!` in `src/lib.rs`.** Verified — `rlsp-yaml-parser/src/lib.rs` opens with `#![deny(clippy::panic)]` then module declarations, no `//!`. A reader opening the crate root sees nothing about what the crate does, the two distinct API layers (`parse_events` vs `load`), or the security model. `rlsp-fmt/src/lib.rs` has a full intro block with Quick Start by comparison. This is the widest orientation gap in the codebase.
- **`README.md` exists** — the original audit was wrong about this. `rlsp-yaml-parser/README.md` is present.
- **`docs/configuration.md` and `docs/feature-log.md` are missing.** The `docs/` directory contains only `benchmarks.md`. Project convention in `CLAUDE.md` mandates configuration and feature-log docs for every `rlsp-<language>` crate. (Whether a pure parser crate needs a `configuration.md` is debatable — it has no user-facing settings — but `feature-log.md` has a clear purpose.)

## Medium-priority findings

### Docs & consistency

- **`rlsp-yaml/src/lib.rs`** — four-line stub linking to GitHub. No description of the library surface (16 `pub mod` declarations), architecture, or entry points. Fix after H7 as a smaller version of the same work.
- **Feature-module orientation in `rlsp-yaml`** — 16 public feature modules (`code_actions`, `code_lens`, `color`, `completion`, `document_links`, `document_store`, `folding`, `hover`, `on_type_formatting`, `parser`, `references`, `rename`, `selection`, `suppression`, `symbols`, `validators`) have no `//!` module doc. Uniform gap; the functions inside are mostly `///`-documented.
- **`rlsp-yaml-parser` module docs** — `encoding.rs`, `error.rs`, `pos.rs`, `limits.rs` have no `//!`. Lower priority than H7.
- **Error-handling convention drift — `SchemaError` uses hand-rolled `impl Display`** — `rlsp-yaml-parser` derives all error types from `thiserror`; `rlsp-yaml` does not declare `thiserror` as a dependency at all, forcing `SchemaError` to hand-implement `Display`. Add `thiserror` and derive.
- **Stale doc comments — mixed findings after verification:**

  **Genuinely stale (flow IS implemented):**
  - `rlsp-yaml-parser/src/event.rs → CollectionStyle` — the doc says "Currently only `Block` is produced; `Flow` will be used when flow sequences and flow mappings are implemented in Task 14." Flow IS implemented; `event_iter/flow.rs` produces `CollectionStyle::Flow`. Rewrite to drop the Task 14 reference and the "currently only" framing.
  - `rlsp-yaml-parser/src/event.rs → CollectionStyle::Flow variant doc` — "(Task 14)" trailing parenthetical is stale.
  - `rlsp-yaml-parser/src/lexer/plain.rs:1249` — test-group label `// Group SPF: scan_plain_line_flow (Task 14)`. Task 14 complete, the function exists (`lexer/plain.rs:453`).
  - `rlsp-yaml-parser/src/event.rs → Event::Alias` — architectural statement is still accurate (loader handles alias resolution), but the `(Task 20)` parenthetical is stale — drop it and keep the explanation.

  **NOT stale — active known-limitation notes that the audit misclassified:**
  - `rlsp-yaml-parser/src/event_iter/step.rs:416-418` — says "Tags (`!`) before `&` are handled in Task 17. IMPORTANT for Task 17: when implementing tag-skip..." This is forward-looking guidance written for a Task 17 implementor. Task 17 has **not** been completed — the note describes how to correctly implement the future fix. Keep as-is or reframe to `// KNOWN LIMITATION (deferred to task N):` but do not delete the substance.
  - `rlsp-yaml-parser/src/lexer.rs:302-308` — `// TODO(architecture): scan_plain_line_block only tokenizes plain scalars. Inline content after --- that starts with ' or " (Task 7)...` This is an active architectural TODO describing a known gap and a fix candidate. The fact that it mentions old task numbers (7, 8, 9, 13) is incidental — the substance is still accurate. Keep or reframe, but don't treat as dead reference.
  - `rlsp-yaml-parser/src/lexer/plain.rs:328` — `In flow context this would additionally exclude flow indicators (Task 13).` — this is a note about a *block-context* function's limitation in flow context. The flow version exists as a separate `scan_plain_line_flow`, so the note is architectural context, not stale. Drop the `(Task 13)` but keep the explanation.
- **`rlsp-yaml/src/server.rs → Settings` struct fields** — most fields have `///` inline comments, but `key_ordering: bool` and `custom_tags: Vec<String>` have none. Minor, uniform gap.

### Structure & type safety

- **`rlsp-yaml/src/schema_validation.rs → validate_string_constraints`** — the `let range = node_range(path, ctx.key_index); ctx.diagnostics.push(make_diagnostic(...))` boilerplate repeats **5+ times** across pattern/min_length/max_length branches (not 3 as originally reported). The cleanest fix hoists `let range = node_range(path, ctx.key_index);` once at the top (it does not depend on the branch) rather than extracting a per-branch helper — each branch differs only in severity, code string, and format arguments.
- **`rlsp-yaml/src/schema.rs → JsonSchema`** — 45+ public `Option<_>` fields including two parallel `exclusive_minimum` / `exclusive_minimum_draft04` pairs (Draft-04 bool form vs Draft-06+ float form for the same concept). Replace each pair with an `ExclusiveBound { BoolFlag(bool), NumericBound(f64) }` enum so the draft distinction is unrepresentable-invalid.

### Performance & idioms

- **`rlsp-yaml-parser/src/event_iter/directive_scope.rs → tag_directives`** — clones every handle/prefix pair out of an owned `HashMap<String, String>` on every `DocumentStart`. Return `&str` pairs (lifetime-tied to `&self`) or store values as `Arc<str>`.
- **`rlsp-yaml/src/server.rs → did_open`/`did_change`** — `uri.clone()` + `text.clone()` to hand to the store followed immediately by `&text` to `parse_and_publish`. Move ownership properly (this overlaps with H1 — bundle with the `parse_and_publish` refactor).
- **`rlsp-yaml/src/schema_validation.rs → get_regex`** — always clones the cached `Regex` on hit. Cache `Arc<Regex>` so hits are refcount bumps.

## Low-priority findings

- **`rlsp-yaml/src/completion.rs → collect_schema_properties` (around line 338)** — verified: `.collect::<Vec<_>>().join("\n")` intermediate collect. Use `itertools::join` or `Iterator::fold`. Cold path.
- **`rlsp-yaml-parser/src/event_iter/directive_scope.rs → DirectiveScope::tag_handles`** — `HashMap<String, String>` could be `HashMap<Cow<'input, str>, Cow<'input, str>>`. Cold per typical YAML but a better pattern.
- ~~**`rlsp-yaml/src/schema.rs → Settings` with 12 `Option<bool>` fields**~~ — **removed 2026-04-11**. Could not reproduce: `Option<bool>` returns zero matches in `schema.rs`. The audit cited the wrong file, type, or field count.
- ~~**`rlsp-yaml/src/formatter.rs → string_to_doc` unnecessary clone**~~ — **removed 2026-04-11**. Likely false positive. `text(value.clone())` clones because `value: &String` is borrowed from a `&Node<Span>` and `text()` consumes a `String`. Removing the clone would require passing the AST by value, which is not how the formatter is called. The clone is required by the ownership model.

## Staleness notes

- **No new stale `file.rs:NNN` references** beyond the three already logged in `project_followup_plans.md` C1. C1 status is unchanged by this audit.
- **Search scope for future audits:** all findings above used function/symbol names rather than line numbers per the project convention. When acting on these findings, `grep` by function name — file positions may have drifted.

## Next steps suggested

1. **Quick wins — independent, uncontroversial:**
   - `find_mapping_colon` dedup (6 copies → 1 shared helper). Largest mechanical dedup in the codebase.
   - `pos_after_line` cleanup (delete walker, promote O(1) `const fn`). Tiny diff, real hot-path win.
   - `MAX_BRANCH_COUNT` / `MAX_DESCRIPTION_LEN` consolidation (2×/3× duplication → one `schema_limits` module).
   - Stale `(Task N)` parenthetical removal on the four genuinely stale comments.
   These could be bundled as a single "small cleanup" plan.

2. **Dedicated "LSP hot-path performance" plan** covering H1 + H4 + H5 as one coherent slice — the settings-snapshot refactor, the `parse_and_publish` structural split, and the `text.lines().collect()` removal belong together because they share the same hot path, the same testing surface, and the same `cargo bench` story.

3. **Rlsp-yaml-parser documentation plan** for H7 — scoped to the crate-level `//!` and `docs/feature-log.md`. Whether `docs/configuration.md` is needed for a crate with no user-facing settings should be clarified with the user first.

4. **Medium and low findings** should accumulate in this file until bundled — none are individually urgent.

5. **Lessons for future audits:**
   - Always check `#[cfg(test)]` boundaries when grepping for patterns like `panic!`, `unwrap`, or `expect`.
   - Count duplicates precisely — undercounts weaken H3, overcounts undermine credibility.
   - "Stale task reference" is not a single category: genuinely stale residue is different from active known-limitation notes that happen to cite task IDs.
   - Before suggesting a fix, grep the codebase for an already-existing implementation of the same idea.
