---
name: Follow-up task queue
description: Remaining items after parser implementation, conformance hardening, migration, and workaround removal
type: project
---

<!-- Only track open items here. Completed work lives in its plan file
     and git history — duplicating it here adds noise and stale state.
     Remove items when their plan is marked Completed. -->

## Open: rlsp-fmt

## Open: rlsp-yaml

<!-- helper-of: convention — an allow-list entry marked `HelperOf` exists because its root
     feature function is also allow-listed as a `TodoRetrofit`. When the root's retrofit plan
     lands, all `HelperOf` entries pointing at that root are removed from the allow-list at the
     same time; they are NOT independent retrofit items and do NOT need their own follow-up
     plans. -->

<!-- Audit-v2 feature-level retrofits — 13 public feature entry points that hand-roll YAML
     scanning instead of consuming the parser AST. Each item below has its signature, violation
     shape, replacement sketch, and the private helpers retired when the root retrofit lands.
     These were surfaced and allow-listed in commit c70f642 under
     .ai/plans/2026-04-18-parser-boundary-audit-v2.md Task 1. -->

- **Retrofit `hover_at` to AST-first** — `hover.rs:31`:
  `pub fn hover_at(text: &str, documents: Option<&Vec<Document<Span>>>, position: Position, schema: Option<&JsonSchema>) -> Option<Hover>`.
  Violation: splits `text` into `lines`, scans line text to find the token at the cursor (`token_at_cursor`), determines which document the line belongs to by counting `---` separators (`document_index_for_line`), and reads indentation and colon positions directly from raw text. The parser AST already has `loc: Span` on every node — cursor resolution should walk the AST by span containment rather than re-scanning text.
  Replacement: remove `text` parameter; accept `documents: &[Document<Span>]` (already partially present but not used for cursor position resolution). Walk the AST to find the deepest node whose `loc` span contains the cursor position; derive path and type from the node itself.
  Helpers retired when this retrofit lands: `document_index_for_line` (hover.rs), `token_at_cursor` (hover.rs), `find_mapping_colon` (hover.rs), `indentation_level` (hover.rs), `sequence_index` (hover.rs).

- **Retrofit `complete_at` to AST-first** — `completion.rs:32`:
  `pub fn complete_at(text: &str, documents: Option<&Vec<Document<Span>>>, position: Position, schema: Option<&JsonSchema>) -> Vec<CompletionItem>`.
  Violation: splits `text` into `lines` and uses a large family of private text-scanning helpers to reconstruct the YAML structural context at the cursor (key path, sibling keys, sequence context, indentation). All structural information is already in the parser AST.
  Replacement: accept `documents: &[Document<Span>]`; walk the AST by span containment to locate the cursor position; derive key path and context from AST node types and structure instead of line-by-line text scanning.
  Helpers retired when this retrofit lands: `build_key_path` (completion.rs), `build_value_key_path` (completion.rs), `collect_present_keys_at_indent` (completion.rs), `classify_cursor` (completion.rs), `suggest_sibling_keys` (completion.rs), `is_in_sequence_item` (completion.rs), `suggest_keys_for_sequence_item` (completion.rs), `collect_current_sequence_item_keys` (completion.rs), `find_current_item_start` (completion.rs), `find_sequence_indent` (completion.rs), `collect_all_sequence_item_keys` (completion.rs), `collect_sibling_keys` (completion.rs), `find_mapping_colon` (completion.rs), `indentation_level` (completion.rs), `document_range` (completion.rs), `suggest_values_for_key` (completion.rs).

- **Retrofit `format_on_type` to AST-first** — `editing/on_type_formatting.rs:11`:
  `pub fn format_on_type(text: &str, position: Position, ch: &str, tab_size: u32) -> Vec<TextEdit>`.
  Violation: scans `text` directly to determine indentation and colon positions on the trigger character's line, then produces text edits based on raw line content rather than the parsed AST structure.
  Replacement: add a `documents: Option<&Vec<Document<Span>>>` parameter; use the AST to determine context at the trigger position (mapping vs. sequence context, current indentation level from parent node spans) rather than scanning line text.
  Helpers retired when this retrofit lands: `leading_spaces` (editing/on_type_formatting.rs), `find_mapping_colon` (editing/on_type_formatting.rs).

- **Retrofit `find_document_links` to AST-first** — `decorators/document_links.rs:38`:
  `pub fn find_document_links(text: &str, base_uri: Option<&Url>) -> Vec<DocumentLink>`.
  Violation: splits `text` into lines and scans each line with hand-rolled text logic to detect URL patterns and `!include`-style directives. The parser AST's `Node::Scalar` nodes already carry the scalar values and their spans — no raw text scanning needed to find URLs in scalar content.
  Replacement: add `documents: &[Document<Span>]`; walk the AST for `Node::Scalar` nodes; inspect their `value` field for URL patterns; derive document links directly from node spans.
  Helpers retired when this retrofit lands: `url_links` (decorators/document_links.rs), `include_links` (decorators/document_links.rs), `is_inside_quotes` (decorators/document_links.rs), `trim_trailing_punctuation` (decorators/document_links.rs).

- **Retrofit `find_colors` to AST-first** — `decorators/color.rs:174`:
  `pub fn find_colors(text: &str) -> Vec<ColorMatch>`.
  Violation: splits `text` into lines and scans each line with text patterns to find hex color strings. The parser AST's `Node::Scalar` nodes already carry scalar values and spans.
  Replacement: add `documents: &[Document<Span>]`; walk the AST for `Node::Scalar` nodes; inspect `value` for color patterns; use `loc` for the result range.
  Helpers retired when this retrofit lands: `value_start_offset` (decorators/color.rs).

- **Retrofit `folding_ranges` to AST-first** — `analysis/folding.rs:10`:
  `pub fn folding_ranges(text: &str) -> Vec<FoldingRange>`.
  Violation: splits `text` into lines and reconstructs document structure entirely through indentation-based text scanning (`collect_indentation_folds`, `collect_document_section_folds`, `collect_comment_block_folds`). Does not consult the parser AST at all.
  Replacement: accept `documents: Option<&Vec<Document<Span>>>` (or `parse_result`); derive fold regions from the AST's node spans — mappings/sequences fold at their `loc`; multi-document sections fold at `---` markers from document spans. Comment-block folding may retain a text carve-out since comments are not in the AST.
  Helpers retired when this retrofit lands: `collect_indentation_folds` (analysis/folding.rs), `collect_document_section_folds` (analysis/folding.rs), `collect_comment_block_folds` (analysis/folding.rs), `find_last_content_line` (analysis/folding.rs), `find_last_content_line_in_range` (analysis/folding.rs), `find_mapping_colon` (analysis/folding.rs).

- **Retrofit `selection_ranges` to AST-first** — `analysis/selection.rs:13`:
  `pub fn selection_ranges(text: &str, documents: Option<&Vec<Document<Span>>>, positions: &[Position]) -> Vec<SelectionRange>`.
  Violation: accepts `documents` but also splits `text` into `lines` and passes them to private helpers that scan text to find document boundaries and map line positions back to document regions. The parser AST's `loc: Span` on every node already encodes all containment boundaries.
  Replacement: remove `text` parameter; use only `documents`; resolve cursor positions purely by span containment walk.
  Helpers retired when this retrofit lands: `selection_range_for_position` (analysis/selection.rs), `find_document_for_line` (analysis/selection.rs), `find_document_end` (analysis/selection.rs).

- **Retrofit `semantic_tokens` to AST-first** — `analysis/semantic_tokens.rs:51`:
  `pub fn semantic_tokens(text: &str) -> Vec<SemanticToken>`.
  Violation: iterates `text` line-by-line to find comments, mapping keys, anchors, aliases, and tags via text pattern matching. Does not consult the parser AST.
  Replacement: accept `documents: Option<&Vec<Document<Span>>>` (or parse result); walk the AST — `Node::Mapping` entries yield keys with spans, `Node::Scalar` nodes with `anchor`/`tag` fields yield their token spans; comment tokens may need a text carve-out since comments are not in the AST.
  Helpers retired when this retrofit lands: `collect_inline_markers` (analysis/semantic_tokens.rs), `char_col_of` (analysis/semantic_tokens.rs), `find_mapping_colon` (analysis/semantic_tokens.rs).

- **Retrofit `document_symbols` to AST-first** — `analysis/symbols.rs:16`:
  `pub fn document_symbols(text: &str, documents: Option<&Vec<Document<Span>>>) -> Vec<DocumentSymbol>`.
  Violation: accepts `documents` but also passes `text` into `split_document_regions` and several line-scanning helpers to reconstruct structure that is already present in the AST nodes and their spans.
  Replacement: remove `text` parameter; derive all symbol names, ranges, and kinds directly from AST node types and `loc` spans.
  Helpers retired when this retrofit lands: `split_document_regions` (analysis/symbols.rs), `find_sequence_item_line` (analysis/symbols.rs), `find_value_end_line` (analysis/symbols.rs), `find_mapping_colon` (analysis/symbols.rs).

- **Retrofit `goto_definition` to AST-first** — `navigation/references.rs:20`:
  `pub fn goto_definition(text: &str, uri: &Url, position: Position) -> Option<Location>`.
  Violation: splits `text` into lines and uses `scan_tokens` to find all anchor/alias tokens through raw text pattern matching. Anchor and alias information is already in the parser AST (`Node::Scalar` has `anchor` field; alias nodes carry the aliased name).
  Replacement: accept `documents: &[Document<Span>]`; walk the AST to collect anchors (nodes with `anchor: Some(name)`) and their spans; resolve aliases by matching name.
  Helpers retired when this retrofit lands: `scan_tokens` (navigation/references.rs), `document_range_for_line` (navigation/references.rs). Note: `scan_tokens` and `document_range_for_line` in `navigation/references.rs` are shared with `find_references` — they are retired when both roots are retrofitted.

- **Retrofit `find_references` to AST-first** — `navigation/references.rs:61`:
  `pub fn find_references(text: &str, uri: &Url, position: Position, include_declaration: bool) -> Vec<Location>`.
  Violation: same shape as `goto_definition` — splits `text` into lines and scans anchor/alias tokens through raw text. Shares private helpers with `goto_definition`.
  Replacement: accept `documents: &[Document<Span>]`; walk AST for all anchor and alias nodes; match by name; use node `loc` spans for result ranges.
  Helpers retired when this retrofit lands: `scan_tokens` (navigation/references.rs), `document_range_for_line` (navigation/references.rs) — same helpers as `goto_definition`; retired together.

- **Retrofit `prepare_rename` to AST-first** — `navigation/rename.rs:21`:
  `pub fn prepare_rename(text: &str, position: Position) -> Option<Range>`.
  Violation: splits `text` into lines and uses `scan_tokens` to locate anchor/alias tokens at the cursor position through raw text scanning. AST nodes carry anchor name and span.
  Replacement: accept `documents: &[Document<Span>]`; walk AST by span containment to find the anchor or alias node at the cursor; return its name span directly.
  Helpers retired when this retrofit lands: `scan_tokens` (navigation/rename.rs), `document_range_for_line` (navigation/rename.rs). Note: shared with `rename`; retired when both roots are retrofitted.

- **Retrofit `rename` to AST-first** — `navigation/rename.rs:50`:
  `pub fn rename(text: &str, uri: &Url, position: Position, new_name: &str) -> Option<WorkspaceEdit>`.
  Violation: same shape as `prepare_rename` — splits `text` into lines and scans anchor/alias tokens through raw text. Shares private helpers with `prepare_rename`.
  Replacement: accept `documents: &[Document<Span>]`; walk AST for all anchors and aliases matching the target name; produce `TextEdit`s from their `loc` spans.
  Helpers retired when this retrofit lands: `scan_tokens` (navigation/rename.rs), `document_range_for_line` (navigation/rename.rs) — same helpers as `prepare_rename`; retired together.

- **Custom tag type annotations** — RedHat's customTags supports `!include scalar`, `!ref mapping` type annotations. Ours is a plain string allowlist — add type annotation support.
- **LSP lifecycle test rstest reduction** — ~34 tests in `lsp_lifecycle.rs` (3000 lines) follow repetitive patterns: "unknown doc returns null" (~8), diagnostic suppression (~10), flowStyle severity (3), max_items_computed (8), settings toggles (~5). Parameterize with rstest to reduce ~500-800 lines. Pure refactoring, no behavior change.
- **`formatIndentSequences` formatter option** — add a `formatIndentSequences: bool` setting (default `true`). When true (default), always produce indented block sequences (`script:\n  - item`). When false, produce indentless sequences (`script:\n- item`). Always normalize — no preserve mode. Formatter currently hardcodes indented style in `formatter.rs:658-669` via `indent()` wrapper.
- **Non-printable unicode character diagnostic** — Parser's comment lexer (`lexer/comment.rs`) and content scanning don't validate characters against `is_c_printable` (YAML 1.2 §5.1). Non-printable/control characters pass through silently. Add LSP diagnostic (`invalidCharacter`, Warning severity) for non-printable characters in comments and content. Security concern: invisible/homoglyph characters could hide malicious content. Parser should preserve them (no data loss); diagnostics should flag them.
- **Formatter fixture gaps: interacting settings combinations** — Fixtures test each formatter setting in isolation but no combinations. Add fixtures for interacting setting pairs (settings that affect the same formatting decision). Derive pairs from `YamlFormatOptions` in `formatter.rs`; see `tests/fixtures/formatter/CLAUDE.md` for guidance.
- **Expand `block_to_flow` code action to support nested block structures** — The action currently refuses nested inputs via `return None` in `code_actions.rs:420`. The `2026-04-18-retrofit-block-to-flow-code-action.md` plan preserved this narrow behavior to keep scope minimal (bug-class elimination, not feature expansion). After the retrofit lands, the AST+formatter path handles nesting automatically — lifting the restriction is cheap. Enhancement plan: remove the pre-check, add tests for nested block-to-flow conversions, confirm the formatter produces correct flow output (e.g., `{a: {b: 1}}`, `[[1, 2], [3, 4]]`).
- **Retrofit `quoted_bool_to_unquoted` to AST+formatter** — Currently span-local text replacement of `"true"` → `true` etc. AST pattern: find the `Node::Scalar`, clone with `style: ScalarStyle::Plain`, re-emit via `format_subtree`. Low complexity. Motivation: architectural consistency — bring all structural scalar-transform code actions under the "one parser, one AST" rule.
- **Retrofit `yaml11_bool_actions` to AST+formatter** — Currently text replacement of `yes`/`no`/`on`/`off` → quoted or converted form. AST pattern: change the scalar's `value` and/or `style`, re-emit via `format_subtree`. Low complexity. Same architectural-consistency motivation.
- **Retrofit `yaml11_octal_actions` to AST+formatter** — Currently text replacement of `0o12` → `10` (or similar). AST pattern: change scalar `value`, re-emit. Low complexity.
- **Retrofit `schema_yaml11_bool_type_actions` to AST+formatter** — Same shape as `yaml11_bool_actions`. Low complexity. Can possibly be combined with the `yaml11_bool_actions` retrofit plan since they share structure.
- **Retrofit `delete_unused_anchor` to AST+formatter** — Currently text replacement removing `&anchor_name` from a line. AST pattern: clone the node with `anchor: None`, re-emit via `format_subtree`. Low complexity; edge case is whether the span covers just the `&name` token or the whole node — the AST version emits the whole node, so the edit range is the node's `loc`.
- **`tab_to_spaces` stays as text replacement** — NOT a retrofit candidate. Tabs are a pre-parse lexical concern (YAML 1.2 §6.1 forbids them for indentation); the parser normalizes or rejects them, so they're not represented in the AST. `tab_to_spaces` is whitespace-cleanup before any structural editing applies. Belongs in the text-edit carve-out category alongside modelines and BOM. Documented here so future audits don't treat this as a missing retrofit.
- **Offer folded block scalar (`>`) as an alternative output form for the `string_to_block_scalar` code action** — The `2026-04-18-retrofit-string-to-block-scalar-code-action.md` plan converts strings to `ScalarStyle::Literal` (`|`) only, preserving the current behavior. Literal preserves newlines verbatim; folded collapses line breaks into spaces (better for prose). After the retrofit lands, offering `>` as a SECOND quick-fix alongside "Convert to block scalar (literal)" is a small UI enhancement: add a `Node::Scalar` clone path targeting `ScalarStyle::Folded`, emit a separate `CodeAction` titled "Convert to folded block scalar". User picks between literal and folded at apply time.
- **Expand `string_to_block_scalar` code action to sequence-item scalars** — The `2026-04-18-retrofit-string-to-block-scalar-code-action.md` plan preserved the current mapping-values-only dispatch (AST walk only looks at `Node::Mapping.entries` values, skipping `Node::Sequence.items`). Symmetric with the `block_to_flow` nested-support enhancement above. A long string like `- "this is a long sequence-item string"` could benefit from block-scalar form too. Enhancement plan: extend the AST walk to also match qualifying `Node::Scalar` values inside `Node::Sequence.items`, add regression tests for sequence-item conversion, verify the formatter produces correct `- |\n  content` output.
- **I5 corpus invariant: validator stability under whitespace re-emit** — Deferred in Move 0 (`.ai/plans/2026-04-18-corpus-invariants-scaffold.md`). For each corpus file and each validator, run the validator on the original text AND on a whitespace-only re-emit of the same document; assert the set of diagnostic codes is identical (ranges may shift). Catches validators whose output depends on whitespace quirks rather than structure.
- **I6 corpus invariant: formatter round-trip** — Also deferred in Move 0. For each corpus file: format the input, parse the formatted output, assert the resulting AST is semantically equivalent to the input's AST (same scalars at the same logical paths, same structure). Catches formatter bugs that produce non-round-tripping output. Already partially covered by the formatter's own fixture tests, but extending to the full corpus closes the "works on fixtures but fails on real files" gap.
- **Expand corpus beyond the 4 seed files** — Move 0 seeded the corpus with `release-plz-workflow.yml`, `kubernetes-deployment.yaml`, `docker-compose.yml`, `github-actions-matrix.yml`. Real-world YAML covers many more shapes: Ansible playbooks, Helm chart templates, GitLab CI pipelines, CloudFormation/CDK YAML, Prometheus alert rules, SOPS-encrypted files, Swagger/OpenAPI specs, Argo CD `Application` manifests, Flux CD `Kustomization`s, Tekton `Pipeline`/`Task` resources. Each adds new coverage. File as one plan per shape, or a batch-add plan for 3-5 at a time. Each new file may surface new I4 failures that flag latent bugs — treat those under the Surprise Failure Protocol.
- **Extend `parser_boundary_audit` to detect private + broader-parameter-name text-scan (audit v2)** — **Sequence priority: execute immediately after `2026-04-18-retrofit-string-to-block-scalar-code-action.md` completes, before the remaining code-action retrofits** (`quoted_bool_to_unquoted`, `yaml11_*`, `delete_unused_anchor`). Rationale: the remaining code-action retrofits would benefit from forward-protection by a working audit. The current audit only catches `pub fn <name>(text: &str, ...)` — two real gaps: (1) it requires `pub`, missing private helpers like the already-retrofitted `flow_map_to_block`, `flow_seq_to_block`, `block_to_flow`, `string_to_block_scalar` (all were `fn`, not `pub fn`, AND took `lines: &[&str]` or `line: &str`, not `text: &str`); (2) it requires the specific parameter name `text`, missing common alternatives like `line`, `lines`, `content`, `source`, `input`. Scope: broaden the detection regex to match `(pub )?fn \w+\s*\(\s*(text|line|lines|content|source|input)\s*:\s*&(?:\[&str\]|str)` or similar; enumerate all private text-handling helpers in `rlsp-yaml/src/`; classify each as either (a) violator needing retrofit (add to allow-list with `TODO(follow-up-plan)` marker) or (b) legitimate text-only helper (add to allow-list with `// carve-out:` justification — `tab_to_spaces`, `find_comment_on_line` in formatter.rs, modeline extraction, BOM detection, whitespace-preserving edit helpers). Per-entry load-bearing verification for each new allow-list entry. **Actual result: 101 entries total, 97 new** (estimate was ~10-15 new; gap due to 13 feature-level roots, 52 private helpers, 18 test-fixture carve-outs). Landed in commit `c70f642` under `.ai/plans/2026-04-18-parser-boundary-audit-v2.md` Task 1.

## Open: rlsp-yaml-parser

- create bindings for python - https://pyo3.rs and typescript (wasm)
