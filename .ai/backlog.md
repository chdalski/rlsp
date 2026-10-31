# Backlog

Open findings and cleanup wishes that no plan holds yet, one
line each: what, where, and where it came from. The lead adds
entries and names them in the completion report. An entry goes
when it becomes a plan or the user drops it.

This is no status log and no home for facts. A missed plan
target goes here only after the user has lowered the target.

## Open

- Flow sequences drop entries after a comment (`[a # c\n  b]` loads as `[a]`) and accept entries without a comma (`["q" "r"]`); `rlsp-yaml-parser/src/event_iter/flow.rs`; libfyaml comparison.
- Alias expansion depth: `max_nesting_depth` does not bound the expanded tree in resolved mode; `expand_node`, `Clone`, `Drop` in `loader.rs`; stack-overflow investigation.
- Iterative loader (`wip/iterative-loader`, adff1074), to discuss with the user; `LoadState::parse_node`; schema queue item 3c.
- Silent data change on valid input: `a:\n: c\n`, `a: 1\n[x]:\n: v\n`, `a: &y\n&k : v\n` make the value belong to the previous key; `-\n  # d\n` drops the empty item; libfyaml comparison, user decision pending.
- `parse_events` omits the empty scalar in an empty document (14 yaml-test-suite cases expect `=VAL :`); the stream conformance test checks only ok/error; a properties-only document before `---` loses its property; libfyaml comparison.
- Valid YAML rejected: `a: 1\n? b: 2\n: c\n` (`block/mapping.rs`), `{a: b\n  c\n}\n`, `{a: x\n  y, b: z}`, `a: !!str\n!!seq [x]: 2\n`, `[x]\t: 2\n`; libfyaml comparison.
- Invalid YAML accepted: `a:\nw\n`, `a:\n[x]\n`, `a: [x]: 2\n`, `a: *y : 2\n`, `&k\n*a\n`, `&a\n&b [x]\n`, `&x\n!!str\n&x v\n`, `? ? a\n[x] :\n`, `?\n!!str\nb: 1\n`, `[x]:2\n`, a `]` at the key's column after a quoted item; libfyaml comparison.
- Inline `--- v` path: `--- v\n# d\nw\n` loses `w`, `--- *a\n` emits the scalar `*a`, `--- [x] [y]\n` loads as one scalar; `block` document start handling; libfyaml comparison.
- A property line before a flow key goes on the key, not the new mapping (`&y\n[x]: 2\n`); `x:\n- &k\ny: 1\n` leaks the anchor to the key; libfyaml comparison.
- `[a\n\n  b]` loads as `"a b"`, should fold to `"a\nb"`; `- a\0\n- b\n` yields events after the error (`block/sequence.rs`); BOM in a plain scalar errors here and loads in libfyaml; libfyaml comparison.
- Multi-line quoted scalars end their span after the line break, and their value tokens have a wrong length (`classify_scalar_node`); span plan.
- Block-scalar code action moves a trailing comment and trailing spaces into the literal value (`editing/code_actions/block_scalar.rs`); span plan.
- `REGEX_CACHE` unbounded and `validate_schema` on the async executor (`src/schema/pattern.rs`); schema queue.
- Path resolver's first pass is unpriced (`resolve_schema_path`, `src/schema/path.rs`); security Medium.
- `httpProxy` and `schemas` VS Code settings have no `scope`/`restrictedConfigurations`, and `configuration.md` misstates the unset `httpProxy`; security Medium.
- SSRF guard checks hostname strings only (`foo.localhost`, DNS rebinding); schema queue.
- Relative and local schemas (`$schema=./schema.json`, `file://`); schema queue.
- Kubernetes detection tests reach the real network without the FakeProxy (`tests/lsp_lifecycle/kubernetes_detection.rs`); new API groups need `BUILTIN_DOTTED_GROUPS` entries; validate-kubernetes-schema-url plan.
- `MAX_LOGGED_SETTING_CHARS` import cycle, `unknownTag` match instead of `has_code`, `loggable_url` cannot strip userinfo from cannot-be-a-base URLs; tolerant-settings plan.
- "after Restart Server" integration test has no baseline assertion (`settings-sync.test.ts`); VS Code settings-sync plan.
- picomatch `?` patterns stay literal in the SchemaStore catalog (11 winget patterns); SchemaStore plan.
- Test consolidation candidates: `file_patterns_match_what_follows_any_slash`, C-6 in `src/completion.rs`, `tests/loader_spans.rs` duplicates, overwrite and deep-nesting fixtures from share-ref-targets; earlier plans.
- Schema cache: no eviction across URLs, no negative cache for rejected schemas; `cargo doc` fails on private links; share-ref-targets plan.
- `configuration.md` diagnostic-code table misses emitted codes (`schemaAdditionalProperties` and others); user queued it.
- "(all required)" completion snippet does not escape names (`schema_key_completions`); security Low.
- `autoWrapFlowStyle` opt-out for `block_to_flow`, only if a user asks; code-action format-config plan.
- Expand the test corpus beyond the 4 seed files (`rlsp-yaml/tests/corpus`); corpus plan.
- Replace `WORKLIST.md` and `SKIP_LIST` with `#[ignore]` (`tests/corpus_invariants/shared.rs`); user preference.
- Python (pyo3) and TypeScript (wasm) bindings for `rlsp-yaml-parser`; user wish.
- `dtolnay/rust-toolchain` refs pin mutable branches; `packageManager` has no integrity hash; dependabot-followup plan.
- `rlsp-yaml-parser/CLAUDE.md` requires `#[non_exhaustive]` on public types, but `Document`, `Node`, `NodeMeta` and others lack it; settle the rule or the types; comment plan design review.
- Formatter fixtures `flow-mapping-long-breaks`, `flow-mapping-bracket-spacing-false-long-breaks`, `interact-bracket-spacing-print-width` and unit cases `flow_map_under_key`, `flow_map_bracket_spacing_false` pin the same layout; comment plan Task 1.
- U+FEFF in a comment body loads though [75] `nb-char` excludes it (libfyaml accepts it too), and `bnf-§6.md` [75] says Strict-conformant; comment plan Task 2.
- `sequence_item_to_doc` in `sequence_render.rs` still copies the prefix matches of `collection_properties`; comment plan Task 3 review.
- `["q"\t# c\n]\n` and `["q"\t, b]\n` are rejected as tab indentation in flow context while libfyaml accepts them; comment plan Task 4 review.
- `? ?\n    k\n` emits no value event for the outer key while libfyaml emits an empty scalar; comment plan Task 4 review.
- `%TAG ! tag:x#y` and `!t#c` load while libfyaml rejects them; [39] `ns-uri-char` includes `#`, so settle it against the spec text in `design-decisions.md`; comment plan Task 4 review.
- `k: [\n  'y' # c\n]\n` is rejected while the same input without the comment loads, and libfyaml rejects both ([197] `]` at column 0); comment plan Task 4 review.
- One long flow-collection line takes quadratic time, about 25 s for 2 MB (suspect `abs_pos` in `event_iter/flow.rs`), a denial-of-service risk for the language server; comment plan Task 4 security review.
- Semantic token lengths count code points, not UTF-16 units; comment plan Task 4.
- Test consolidation candidates: `trailing_comment_after_plain_scalar`, `trailing_comment_after_mapping_value`, `trailing_comment_leading_space_preserved`, `trailing_comment_after_sequence_entry`, `trailing_comment_no_trailing_newline` in `tests/smoke/comments.rs` into one table; `comment_line_produces_comment_token`, `comment_line_with_indent_starts_at_hash`, `indented_comment_position_from_event_span` into `comment_token_position_matches_event_comment_span`; comment plan Task 4 test review.
