# Backlog

Open findings and cleanup wishes that no plan holds yet, one
line each: what, where, and where it came from. The lead adds
entries and names them in the completion report. An entry goes
when it becomes a plan or the user drops it.

This is no status log and no home for facts. A missed plan
target goes here only after the user has lowered the target.

Each entry sits in the section for its kind. Related entries
share a parent line with one sub-item each.

## Security

- Alias expansion depth: `max_nesting_depth` does not bound the expanded tree in resolved mode; `expand_node`, `Clone`, `Drop` in `loader.rs`; stack-overflow investigation.
- `REGEX_CACHE` unbounded and `validate_schema` on the async executor (`src/schema/pattern.rs`); schema queue.
- Path resolver's first pass is unpriced (`resolve_schema_path`, `src/schema/path.rs`); security Medium.
- `httpProxy` and `schemas` VS Code settings have no `scope`/`restrictedConfigurations`, and `configuration.md` misstates the unset `httpProxy`; security Medium.
- SSRF guard checks hostname strings only (`foo.localhost`, DNS rebinding); schema queue.
- "(all required)" completion snippet does not escape names (`schema_key_completions`); security Low.
- One long flow-collection line takes quadratic time, about 25 s for 2 MB (suspect `abs_pos` in `event_iter/flow.rs`), a denial-of-service risk for the language server; comment plan Task 4 security review.
- Formatter output grows with depth times `tab_width` (a 250-level flow sequence prints about 250 times its input), and time grows with the square of the item count; `tab_size` and `print_width` come from the client unchecked; medium, nesting plan Task 2 security review.
- `fetch_schemastore_catalog` in `rlsp-yaml/src/schema.rs` calls `fetch_json` without `check_json_depth` or the raw budget, so only the 5 MiB cap and serde's depth limit of 128 apply (fixed URL, non-recursive consumer); security Low, nesting plan Task 11 security review.
- CVE-2026-118764 (minimatch) is allowlisted in the VS Code extension's `ignoreCves` until `vscode-languageclient` runs with minimatch 11.0.1 or later or rlsp-yaml lets a pattern reach the client's minimatch; reason and triggers in the `Audit dependencies` step comment of `.github/workflows/vscode-extension.yml`; glob override plan.
- The actions in `.github/workflows/vscode-extension.yml` are pinned to mutable major tags, not commit SHAs, including the publish job that uses `VSCE_PAT`; pre-existing, security Medium; glob override plan Task 2 security review.

## Bugs

- `parse_events` omits the empty scalar in an empty document (14 yaml-test-suite cases expect `=VAL :`); the stream conformance test checks only ok/error; libfyaml comparison.
- Valid YAML rejected: `{a: b\n  c\n}\n`, `{a: x\n  y, b: z}`, `[x]\t: 2\n`; libfyaml comparison.
- Invalid YAML accepted: `a:\nw\n`, `a:\n[x]\n`, `a: [x]: 2\n`, `a: *y : 2\n`, `&x\n!!str\n&x v\n`, `? ? a\n[x] :\n`, `?\n!!str\nb: 1\n`, `[x]:2\n`, a `]` at the key's column after a quoted item; libfyaml comparison.
- Inline `--- v` path: `--- v\n# d\nw\n` loses `w`, `--- x\n  y\n` gives two scalars, and `--- x\n  @y\n` (also `,y`, `` `y ``, `%y`) is an error since comment plan Task 11 while libfyaml yields `x @y`; `block` document start handling; libfyaml comparison.
- A property line before a flow key goes on the key, not the new mapping (`&y\n[x]: 2\n`); libfyaml comparison.
- `- a\0\n- b\n` yields events after the error (`block/sequence.rs`); BOM in a plain scalar errors here and loads in libfyaml; libfyaml comparison.
- Multi-line quoted scalars end their span after the line break (`try_consume_single_quoted`, `try_consume_double_quoted` in `lexer/quoted.rs`), so their value tokens have a wrong length (`classify_scalar_node`); span plan, comment plan Task 7.
- Block-scalar code action moves a trailing comment and trailing spaces into the literal value (`editing/code_actions/block_scalar.rs`); span plan.
- picomatch `?` patterns stay literal in the SchemaStore catalog (11 winget patterns); SchemaStore plan.
- U+FEFF in a comment body loads though [75] `nb-char` excludes it (libfyaml accepts it too), and `bnf-§6.md` [75] says Strict-conformant; comment plan Task 2.
- `["q"\t# c\n]\n` and `["q"\t, b]\n` are rejected as tab indentation in flow context while libfyaml accepts them; comment plan Task 4 review.
- `? ?\n    k\n` emits no value event for the outer key while libfyaml emits an empty scalar; comment plan Task 4 review.
- `%TAG ! tag:x#y` and `!t#c` load while libfyaml rejects them; [39] `ns-uri-char` includes `#`, so settle it against the spec text in `design-decisions.md`; comment plan Task 4 review.
- `k: [\n  'y' # c\n]\n` is rejected while the same input without the comment loads, and libfyaml rejects both ([197] `]` at column 0); comment plan Task 4 review.
- Semantic token lengths count code points, not UTF-16 units; comment plan Task 4.
- `format_subtree` in `formatter.rs` indents a literal's blank lines with `base_indent`, so code actions produce whitespace-only lines (the value stays); comment plan Task 6 review.
- An empty root document in the middle prints a blank line (`---\n\n---`) in `render_documents`, and `a:\n  # c\n` prints `a: ` with a trailing space; comment plan Task 6.
- The server reads the modeline and suppressions with `str::lines()`, so they have no effect with `line_ending = cr`; comment plan Task 6 security review.
- The formatter drops `%YAML` and `%TAG` directives, and blank-line pairing then loses every blank line in the rest of the file; comment plan Task 6 security review.
- An own-line comment between `k:` and its scalar value prints after the entry, so a suppression comment there hits the neighbour; comment plan Task 7 security review.
- `delete_anchor` misindents a block scalar's content lines; comment plan Task 7.
- `- ` keeps a trailing space for null items and nested sequences; comment plan Task 7 test review.
- `? ? a\n  : 1\n: v\n` is misread by the parser (the formatter avoids the form), and `a:\n!t : 1\n` belongs to the silent data change entry; comment plan Task 8 security review.
- `? b\n` prints `b: ` with a trailing space, as `- ` does for nested sequences; comment plan Task 9 review.
- Blank-line restoration loses every following blank line after an entry that `format_remove_duplicate_keys` removed; comment plan Task 9 test review.
- Marker line and flow, rejected here and loaded by libfyaml: `--- :`, `--- : a`, `--- &a {a: 1}`, `--- &a [a: 1]`, `--- &a !t {a: 1}`, `--- &a {a: *a}`, `[!!null , b]`, `[&a , b]`, `[!t , b]`, `{?}`, `[?]`, implicit collection keys over the length limit; comment plan Task 11 review.
- Flow keys differing from libfyaml: multi-line implicit keys `[ {a: 1,\n b: 2} : v ]` and `[ [a,\n b] : v ]` load here; `[ ? a, b: c ]` yields item `a`; `{ ? {a: b}, c: d }` omits the first entry's empty value; `? ? a` omits the null tag on its empty value, which `data_signature` hides; comment plan Task 11 review.
- `{ a: b: c }` and `{ ? a : b : c }` load as `{ a: b, c:  }` while libfyaml reports a missing comma; `a: 1\n? ?\n`, `- :\n  - x\n  ? ?`, and `- \ta: 1` are rejected while libfyaml accepts them; `? \ta\n: \tb` is accepted while libfyaml rejects it; comment plan Task 12 review.
- `validate_unused_anchors` ranges its diagnostic over the anchored node's `loc`, not `&name`, so empty nodes and collections get a later line; other code actions that read that range are suspect; comment plan Task 12.
- `format_yaml` takes about 2.4 times as long as release 0.14.0 on a 1.7 MB file (230 ms against 94 ms), since the comment plan added reloads and passes; profile it; comment plan measurement.
- The directive error reports one line late (`a: 1\n...\n%a\n` at 4:0), `%` in flow context says "not yet supported", and `a: 'x\n%y'` loads while `a: "x\n%y"` errors; parser plan Task 3.
- On an error in the key's own `:` comment (`*a : # \x07`), the alias or flow events queued before it are no longer emitted, because `next()` in `base.rs` clears the queue; error kind and position stay; parser plan Task 4 review.
- `- &a\n  *a\n` emits an alias and an empty `&a` item, while libfyaml errors; `- >\n    \t\n` loads `   \t\n` while libfyaml loads `\t\n`, since a line of spaces and a tab counts as empty for indentation; parser plan Task 5 review.
- `[a,\n &x]` prints `[a, &x !!null ""]`: a tag appears in the output; parser plan Task 9 review.
- The first single-line plain scalar is bounded by input size only, not `MAX_SCALAR_LEN`, and blank lines between keys are lost when a value gets fewer lines (`a: x\n  y\n\nb: 1\n`); parser plan Task 6 review.
- A core tag `!!map` on a block key is dropped on format, with the same data; parser plan Task 14 review.
- `range_formatting` in `rlsp-yaml/src/server.rs` joins `formatted.lines()` with `\n`, so editorconfig `end_of_line = crlf` and `insert_final_newline = false` have no effect there; nesting plan Task 3 review.
- `scan_scalar_for_colors` in `rlsp-yaml/src/decorators/color.rs` shifts the range of a color in a quoted scalar one character left (`primary: '#ff0000'` gives 9..16, not 10..17), so a colorPresentation edit overwrites the quote; `tests/lsp_lifecycle/document_color.rs` pins today's range; nesting plan Task 4 review.

## Tests

- `document_symbol_request` in `rlsp-yaml/tests/lsp_lifecycle/nesting_depth.rs` duplicates the helper in `navigation.rs`, and its `server()` overlaps `helpers::start_server` (it drops the log instead of collecting it); nesting plan Task 9 and Task 11 test reviews.
- The stdio framing (`read_one_message`, `read_content_length`, `send`, `recv_matching`, `is_response_to`) exists in both `rlsp-yaml/tests/claude_code_stdio_smoke.rs` and `tests/lsp_lifecycle/stack_depth.rs`; extract it to a shared `tests/common` module; nesting plan Task 11 review.
- The release runs in `rlsp-yaml/tests/lsp_lifecycle/stack_depth.rs` fetch no 49-level schema and do not assert that the validator reaches its depth limit at 512 levels; only the debug and stdio runs do; nesting plan Task 11 review.
- Kubernetes detection tests reach the real network without the FakeProxy (`tests/lsp_lifecycle/kubernetes_detection.rs`); new API groups need `BUILTIN_DOTTED_GROUPS` entries; validate-kubernetes-schema-url plan.
- "after Restart Server" integration test has no baseline assertion (`settings-sync.test.ts`); VS Code settings-sync plan.
- Expand the test corpus beyond the 4 seed files (`rlsp-yaml/tests/corpus`); corpus plan.
- Replace `WORKLIST.md` and `SKIP_LIST` with `#[ignore]` (`tests/corpus_invariants/shared.rs`); user preference.
- Formatter fixtures `flow-mapping-long-breaks`, `flow-mapping-bracket-spacing-false-long-breaks`, `interact-bracket-spacing-print-width` and unit cases `flow_map_under_key`, `flow_map_bracket_spacing_false` pin the same layout; comment plan Task 1.
- The table `flow_and_alias_keys_after_finished_entry_emit_libfyaml_events` in `tests/error_reporting.rs` has over 170 cases on several subjects; parser plan Task 4 test review.
- `smoke/sequences.rs` `fast_path_skipped_bare_dash` duplicates `dash_followed_by_newline` in `sequence_empty_item_emits_empty_plain_scalar`; parser plan Task 5 test review.
- Test cleanup in `src/loader.rs`: `document_of` and the inline `DocumentStart` cases could use `document_start()`, the stray-end cases of `an_event_stream_that_does_not_close_what_it_opened_is_an_error` could assert `Syntax`, and `an_unbalanced_event_stream_is_an_error` could use `assert_syntax_error`; parser plan Task 7 review.
- `lockfileRules` in `rlsp-yaml/integrations/vscode/src/overrides.test.ts` is a tuple array checked by a coverage test; a `Record<GuardedPackage, LockfileRule>` would enforce a row per package at compile time; vscode override plan Task 1 review.
- `has_mapping_with_odd_node_count` in `rlsp-yaml/tests/formatter_keeps_text.rs` repeats `check_well_formed` of `rlsp-yaml-parser/tests/support/mod.rs` across the crate boundary; parser plan Task 12 review.
- `? !!str\n!zz : v\n? &x\n` has no case in `rlsp-yaml/tests/formatter_keeps_data.rs`; parser plan Task 12 review.
- `a_dash_after_a_tag_does_not_load` in `rlsp-yaml/tests/formatter_keeps_comments.rs` does not pin `--- !t - b`, `&a !t - b`, `!t\t- b`, `!t -`, and `- !t - b`, and `: !\n?\n! : v\n` is not in `libfyaml_events.rs`; parser plan Task 13 review.
- `anchor_followed_by_inline_dash_returns_error` in `rlsp-yaml-parser/tests/smoke/anchors_and_aliases.rs` is a weaker duplicate of the `&a - b` case in `tests/error_reporting.rs`, and `tests/smoke/probe_dispatch.rs` refers to it; removal candidate, parser plan Task 13 review.
- A comment before the `:` of an inner entry of a block key has no pinned placement in `rlsp-yaml/tests/formatter_keeps_comments.rs`; parser plan Task 14 review.
- Test consolidation candidates:
  - `file_patterns_match_what_follows_any_slash`, C-6 in `src/completion.rs`, `tests/loader_spans.rs` duplicates, overwrite and deep-nesting fixtures from share-ref-targets; earlier plans.
  - `trailing_comment_after_plain_scalar`, `trailing_comment_after_mapping_value`, `trailing_comment_leading_space_preserved`, `trailing_comment_after_sequence_entry`, `trailing_comment_no_trailing_newline` in `tests/smoke/comments.rs` into one table; `comment_line_produces_comment_token`, `comment_line_with_indent_starts_at_hash`, `indented_comment_position_from_event_span` into `comment_token_position_matches_event_comment_span`; comment plan Task 4 test review.
  - the `*_reaches_no_node` cases of `comment_between_key_and_value_placement` in `src/loader.rs` with the `end_comments` cases in `tests/loader_comments.rs`; the walkers `nodes_with_leading_comments` and `comment_places`; comment plan Task 5 test review.
  - the triple `comment-*` formatter fixtures and two `blank-line-*` fixtures with identical content and no blank line; comment plan Task 6 test review.
  - the weak `anchor_with_trailing_comment_preserved` and the similar `comment-line-after-plain-*` fixtures; comment plan Task 7 test review.
  - `dedup.rs`: the misleadingly named `dedup_removed_entry_with_trailing_comment_no_crash`, and groups C and F into one `rstest` table; comment plan Task 9 test review.
  - `analysis/folding.rs`: the guarded, redundant `should_fold_consecutive_comment_block`, `should_return_empty_for_comment_only_document` (no document loads), and `should_not_fold_single_comment_line` and `should_fold_comment_block_at_end_of_file` into the new table; comment plan Task 10 test review.
  - repeated `assert_data_kept` calls in four tests, and the overlap of groups O and P in `tests/smoke/documents.rs`; comment plan Task 11 test review.
  - `document_comment_lists`, `missing_document_comment`, `invented_comment`, and `formatter_keeps_the_data_of_suite_case` overlap the new sweep, and `formatter_idempotency.rs` keeps its own copy of the suite loader; comment plan Task 12 test review.
  - `tests/smoke/conformance.rs`: duplicate `cf9_27na` and `yaml27na`, and five trivial `--- text` tests into one table; parser plan Task 2 test review.
  - the two `%` tables in `tests/smoke/scalars.rs`, the loader table in `tests/loader.rs`, and a shared `first_error` helper in `tests/smoke/main.rs`; parser plan Task 3 test review.
  - `formatter_keeps_comments.rs`: `a_second_colon_in_a_flow_sequence_entry_does_not_load`, `a_missing_comma_in_a_flow_collection_does_not_load`, and `a_block_entry_after_tab_indentation_does_not_load` share a body; parser plan Task 8 test review.
  - the stack-thread helper exists three times (`on_stack` in `src/loader.rs` and `tests/robustness.rs`, `on_small_stack` in `tests/loader_comments.rs`), and three older depth tests in `tests/robustness.rs` overlap `the_limit_holds_on_a_two_mib_stack`; nesting plan Task 1 review.
  - `rlsp-yaml/tests/editorconfig_integration.rs`: A2 only checks its local copy `make_options_from_ec`, and B1 to B3 and C1 to C3 repeat the `line_ending_*` and `insert_final_newline_*` unit tests of `formatter.rs`; nesting plan Task 3 test review.
  - fold `collection_keys_nested_as_deep_as_the_parser_accepts_keep_their_data` (400 levels) into the rstest `explicit_keys_nested_as_deep_as_the_parser_accepts_keep_their_data` in `rlsp-yaml/tests/formatter_keeps_data.rs`; nesting plan Task 6 review.
  - `rlsp-yaml`: `should_not_panic_on_deeply_nested_yaml` and the three default-nesting tests in `src/parser.rs` (partly repeated by the rstest case 256), and the four `get_max_items_computed_*` tests in `src/server.rs`; nesting plan Task 8 test review.
  - `should_publish_no_schema_diagnostics_when_schema_exceeds_depth_limit` in `rlsp-yaml/tests/lsp_lifecycle/schema_fetch.rs` partly overlaps `schema_beyond_the_json_depth_limit_is_refused` in `stack_depth.rs`; nesting plan Task 11 test review.

## Cleanup

- `MAX_LOGGED_SETTING_CHARS` import cycle, `unknownTag` match instead of `has_code`, `loggable_url` cannot strip userinfo from cannot-be-a-base URLs; tolerant-settings plan.
- Schema cache: no eviction across URLs, no negative cache for rejected schemas; `cargo doc` fails on private links; share-ref-targets plan.
- `sequence_item_to_doc` in `sequence_render.rs` still copies the prefix matches of `collection_properties`; comment plan Task 3 review.
- `prints_nothing` in `formatter.rs` repeats the rule of `node_to_doc.rs` for when a tag prints; comment plan Task 6 review.
- `explicit_key_to_doc` is `pub(super)` but only used in its own file; comment plan Task 7 review.
- `has_properties: bool` and `explicit_key: bool` in `mapping_render.rs` go against `lang-rust.md` (enums over boolean parameters); comment plan Task 8 review.
- `flow.rs` builds the same empty scalar from the pending properties at three sites, and `clear_pending_properties` in `base.rs` has one caller left; comment plan Task 12 review.
- `forced_explicit_key` in `mapping_render.rs`: the clause for a collection key with properties after a value without text is no longer needed for the data (`a: !u\n!t [x]: 1\n`, `a: \n!t [x]: 1\n`), so the output stays longer than needed; parser plan Task 10 review.
- Formatter cleanups after the frame split: `explicit_value_line` in `mapping_render.rs` takes `value_is_empty` that it could compute, the doc comment of `render_node` describes the flow quoting rule now in `scalar_to_doc`, and `scalar_to_doc` could take `Option<&str>` for its tag; nesting plan Task 2 review.

## Documentation

- `configuration.md` diagnostic-code table misses emitted codes (`schemaAdditionalProperties` and others); user queued it.
- The doc comment on `consume_leading_comments` in `loader/stream.rs` still says block-level and own-line only, which no longer holds for `- # c`, `--- # c`, and the document-level callers; comment plan Task 5 review.
- `rlsp-yaml-parser/docs/architecture.md`, Resolved bullet: the added lines lack the list item's indent; comment plan Task 5 review.
- The comment in `handle_plain_scalar_inline` in `lexer.rs` names `*`, `%`, `{`, `[`, which no longer reach it, and the header of `tests/formatter_conformance.rs` says comments at node positions do not print yet; comment plan Task 11 review.
- `docs/architecture.md` lists `try_consume_double_quoted(parent_indent)`, but the parameter is `block_context_indent`; parser plan Task 2 review.
- `rlsp-yaml-parser/docs/feature-log.md`, "Literal Stream Character Validation", omits the implicit-key exception, and the Tests line of `bnf-§5.md` [1] omits Group IV-E of `tests/smoke/char_validation.rs`; parser plan Task 3 review.
- `rlsp-yaml-parser/docs/feature-log.md`: "and so are `&p1` followed by ..." reads awkwardly; parser plan Task 8 review.
- `rlsp-yaml/docs/feature-log.md` does not name the changed formatter output (no `?` on an empty, tagged, or anchored key after an empty value, or on a first key inside a value); parser plan Task 10 review.
- The doc comment on `settle_collection_styles` in `rlsp-yaml/src/editing/formatter/flow_comments.rs` writes `enforce_block`, which is no longer a parameter there; parser plan Task 14 review.
- The doc comment of `parse_node` in `rlsp-yaml-parser/src/loader.rs` says each level holds a frame of both collection loops; it holds the one that matches its collection; nesting plan Task 1 review.
- `rlsp-yaml-parser/docs/architecture.md`: in "can build trees deeper than the limit, and it does not bound code that walks the loaded tree" the "it" reads as alias expansion, not the limit; nesting plan Task 7 review.
- The core-limit comment in `.devcontainer/docker-compose.yml` lets "then" in "unless a process lowers its own limit to 0; the kernel then skips a piped handler" read as the 0 case; core-limit plan Task 1 review.
- The `Audit dependencies` step comment in `.github/workflows/vscode-extension.yml` calls the other minimatch copies dev-only, but eslint and `@vscode/test-cli` share the bundled 10.2.6 copy; "its other dependents (mocha, eslint, `@vscode/test-cli`, c8) are dev-only" is precise; glob override plan Task 2 review.

## Tooling & CI

- The Node version comments in `.github/workflows/vscode-extension.yml` and `.github/workflows/coverage.yml` say to revisit when Node 26 becomes LTS (2026-10-28), a date that has passed; glob override plan design review.
- `dtolnay/rust-toolchain` refs pin mutable branches; `packageManager` has no integrity hash; dependabot-followup plan.

## Open decisions

- Flat arena AST as a possible goal for a 1.0 of `rlsp-yaml-parser`: about 30 to 50% faster `load()` and far less memory in the spike, no depth limit from the stack; `rlsp-yaml` would need a port (479 `Node` patterns in 40 files, an edit API for the formatter); branch `spike/flat-ast` (a09dad39), report `.ai/reports/2026-11-02-loader-depth-spikes.md`; user request.
- Relative and local schemas (`$schema=./schema.json`, `file://`); schema queue.
- `autoWrapFlowStyle` opt-out for `block_to_flow`, only if a user asks; code-action format-config plan.
- Python (pyo3) and TypeScript (wasm) bindings for `rlsp-yaml-parser`; user wish.
- `rlsp-yaml-parser/CLAUDE.md` requires `#[non_exhaustive]` on public types, but `Document`, `Node`, `NodeMeta` and others lack it; settle the rule or the types; comment plan design review.
- "Convert flow sequence to block style" drops scalars in a flow sequence nested in a multi-line flow collection with comments, and diagnostics change when such a collection turns block (corpus invariants I3, I4, I11), so flow comment positions stay out of `comment-positions.yaml`; comment plan Task 12, for the user's decision.
- Parser conformance to the YAML spec and libfyaml, for the user's decision:
  - Marker line, accepted here and rejected by libfyaml: `--- "a": 1`, `--- 'a': 1`, `--- !t a: b` load a mapping, against the doc comment on `handle_marker_inline`; parser plan Task 2 review, for the user's decision.
  - A tab after the indentation before a property is valid per [66] and [69] and loads here, while libfyaml rejects it; recorded in `design-decisions.md`, for the user's decision; comment plan Task 12.
  - Differing from libfyaml, unchanged by the parser plan: `- x\n\ty\n` and `--- |\n\ty\n` error here and load there; `a: x\n?\n- z\n`, `a:\n- x\n-\n`, and the under-indented `a: 'q\n\nq'\n` load here and error there; `a: x\n?\nz\n` errors here and loads there; parser plan Task 2 review, for the user's decision.
  - A non-printable character in an implicit key still loads (`\x07b: 2`, `a:\n  \x07b: 2`, `  \uFEFFb: 2`, `- \0b: 1`), against [1], [27], and [34]; `design-decisions.md` records it as open; parser plan Task 3 review, for the user's decision.
  - Rejected here, loaded by libfyaml since parser plan Task 4: a block collection on the `:` line of an empty key (`a:\n: - b`, `[x]:\n: - a`, `a:\n: b: c`, `a:\n: : v`) and a property line below a nested empty-key entry (`*a :\n :\n &y\n`); `design-decisions.md` [194] marks it pending; for the user's decision.
  - Differing from libfyaml, unchanged by the parser plan: `: ? b` and `a: ? b` load here against the [194] reading; `k: *b : c`, `k: [x]: c`, `*a :\t- x`, `[x]:\t- x`, `a: *z :`, and `a: [q]: v` load here and error there; `&y\n[x]: v`, `!t &a\n[a]: b`, and `&a !t\n{a}: b` leave properties on the key instead of the mapping; `[x]:2` differs; parser plan Task 4 review, for the user's decision.
  - Bare dash, differing from libfyaml: `k:\n-\n \n    \n`, `  k:\n  -\n   \n`, and `k:\n- &a\n-\n  \n` load here and error there; `-\nb\n`, `-\n# c\nw\n`, `-\n  # c\nw\n`, `-\n\nw\n`, `- -\n  b\n`, and `- -\n  # c\n  b\n` error here and load there, and tests pin that per [185] and [197]; parser plan Task 5 review, for the user's decision.
  - Flow, differing from libfyaml: a flow mapping value does not continue over lines, so `{k: v\n w}`, `{a: b\n\n c}`, `{a: b\n &x c}`, `{a: b\n "c"}`, and `{a: b\n !t}` error here ([135], [141]); `[? a # c\n :b]` errors here ([126], [127], [138]); `k: [a\n\nb]`, `k: [a\n!t b]`, `[a\n\n\tb]`, `[a\n\t&x b]`, `{a\n\t*x: 1}`, `{"a"\n :p}`, `{a,\n ?}`, and `[a,\n ?]` error here; `[a\n\n b: 1]` loads a pair against [154], as libfyaml does; `[\n !t : ]`, `{a: b, !t :&x }`, and `[a,\n !t : 1]` differ; parser plan Tasks 6 and 9 reviews, for the user's decision.
  - Flow, loaded here and rejected by libfyaml since or through parser plan Task 9: `[&k a\n w: 1]` and `[&k a\n *x: 1]` ([133], [154]), and `[a\n x:]`, `{a\n x:}`, `{a\n b\n c:}`; parser plan Task 9 review, for the user's decision.
  - `parse_events` yields no error for `&k\n*a\n` and its variants: it emits the alias and an anchored empty scalar, and only `load()` reports the error, at a position after the last line; `- &k\n  *a\n` still loads as two items; libfyaml errors at the alias; parser plan Task 7 review, for the user's decision.
  - `&a !t\n!z [a]:2\n` and `a: &a !t\n  !z [a]:2\n` load since parser plan Task 8 while libfyaml errors, the `[x]:2` family; `&p1\n!q1 |\n  t\n` and `!p1\n&q1 >\n  t\n` load while libfyaml errors, with no property lost; parser plan Task 8 review, for the user's decision.
  - Explicit keys, differing from libfyaml: `?\n  ?\ta\n: v\n`, `x:\na\n? ?\n`, `- ?\n  !t\n  ? x: 1\n`, `? ?\n  -\n`, and `?\n!t\n- ?\n` load here and error there, and `? ?\n  a\n` and `?\n  ?\n  a\n` error here and load there ([66], [190], [201]); parser plan Task 12 review, for the user's decision.
  - Properties before an inline collection, differing from libfyaml: `!t ? b`, `&a ? b`, `a: !t ? b`, `a: !t ? :`, `a: !t b: c`, `a: &x b: c`, and `!é b` load here and error there ([200]); parser plan Task 13 review, for the user's decision.
