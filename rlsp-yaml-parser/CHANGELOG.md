# Changelog


## [0.11.0] - 2026-05-13

### Documentation

- Add Conventions section with non-exhaustive rule (cd6e80c)
- Update benchmark numbers from 2026-05-13 baremetal run (806c1a2)

### Features

- Mark error types non_exhaustive (90e5b7c)
- Introduce ErrorKind enum on event-stream API (bd31fec)
- Propagate ErrorKind through LoadError::Parse (4763e70)

## [0.10.0] - 2026-05-05

### Bug Fixes

- Detect BOM-less UTF-32 input per YAML §5.2 table (f16a0cf)
- Enforce 1 MiB cap on all quoted-scalar paths (0c19f25)
- Reject non-c-printable characters in literal stream input (666e2f2)
- Validate directive names and parameters against ns-char (51cdfdd)
- Validate tag prefix against ns-uri-char and reject empty shorthand suffix (4a6d2ee)
- Enforce s-indent(n) on quoted scalar continuation lines (1ed7c94)
- Reject signed octal and hex integers under Core schema (2865a74)
- Stop %TAG prefix from absorbing trailing comments (9056eed)
- Enforce verbatim tag admissibility and separator (02babe6)
- Validate resolved tag URI after handle+suffix concatenation (0a6f09e)
- Point error positions to precise offending byte (4eaed0f)
- Add position fields to LoadError variants (4f4a840)
- Reject double BOM at stream start (6b8219e)

### Documentation

- Clarify flow-pair single-line check rationale (4f03136)
- Create conformance documentation structure and README (e37bf1e)
- Add per-chapter BNF conformance entries (§5–§9) (665e506)
- Add Phase 2 prose findings and design decisions (cb59538)
- Add inline conformance doc comments and create crate CLAUDE.md (cc59519)
- Add agent instruction comment to Conformance Sync section (5f83ef6)
- Add public conformance status declaration to README (a226202)

### Performance

- Add c-printable pre-scan flag to skip per-scalar validation (5ffda4b)

## [0.6.1] - 2026-04-27

### Bug Fixes

- Revert manual version bumps — release-plz manages versions (b96bedc)

## Bug Fixes

- Accept BOM at document-prefix positions per YAML 1.2 §5.2 (49a36cb)
- Reject underscore in named tag handle names ([92]) (589a5ec)
- Suppress resolver-injected tags on empty scalars (34bcc58)

## Documentation

- Cache YAML 1.2.2 spec and scaffold conformance audit (f234708)
- Draft §3/§4/§5 conformance entries [1]–[62] (752e622)
- Verify §3/§4/§5 conformance entries against cached spec (3e23f94)
- Draft §6 conformance entries [63]–[103] (6b67752)
- Verify §6 conformance entries against cached spec (2c73f86)
- Draft §7 conformance entries [104]–[161] (a2982ba)
- Verify §7 conformance entries against cached spec (29fd46c)
- Draft §8 conformance entries [162]–[201] (9428165)
- Verify §8 conformance entries against cached spec (cc961f8)
- Draft §9 conformance entries [202]–[211] (0d5b42b)
- Verify §9 conformance entries against cached spec (066dd76)
- Draft §10 conformance entries (8ca0224)
- Verify §10 conformance entries and append Summary (562e013)
- Update conformance doc for BOM-between-documents fix (894e14b)
- Add Strict (security-hardened) sub-class to audit methodology (631b34f)
- Reclassify hex-escape findings as Strict (security-hardened) (f2cce24)
- Flip [154]/[155]/[192]/[193] to Conformant after 1024-char limit (8bee30d)
- Flip [92] c-named-tag-handle to Conformant after underscore fix (4fc9b91)
- Update conformance doc and feature-log for §10 schema resolution (84fc75c)

## Features

- Enforce 1024-char implicit key limit in block context (cc7b6ba)
- Enforce 1024-char limit for flow-context implicit keys ([154], [155]) (b7b6bcc)
- Add Schema enum and §10 tag resolution infrastructure (5296aa1)
- Wire §10 schema tag resolution into the loader (d640dd1)
- Add JSON and Failsafe schema resolution variants (60c095c)
- Make Schema::Core the loader default and remove load_with_schema (073f128)

## Refactoring

- Hoist module-scope use/mod to header per import-placement rule (698263d)
- Hoist fn-body use statements per import-placement rule (c63d70f)
- Reorder sub-module use/mod headers per import-placement rule (543cd93)

## Features

- Expose anchor_loc span on node events (265cb5a)
- Expose anchor_loc span on AST nodes (0ca3083)
- Expose tag_loc span on node events (521e3b5)
- Propagate tag_loc through AST nodes (a7870c9)

## Bug Fixes

- Rewrite while_let_loop sites and remove trailing commas (60202c2)

## Documentation

- Refresh benchmarks.md with 2026-04-16 baremetal data (3b489b1)

## Performance

- Inline loader helpers node_end_line and is_block_scalar (9370579)
- Short-circuit trailing-comment detection with peek guard (d9afbdf)
- Inline loader stream helpers (3f493a8)
- Skip anchor-subtree clone in Lossless mode (a506589)
- Replace format! with direct push for comment prefix (d586012)
- Use memchr2 for value-indicator scan (8097aa5)
- Wrap leading_comments in Option<Vec<String>> (e812232)
- Inline consume_leading_comments fast path and with_hash_prefix (3bec2da)

## Bug Fixes

- Preserve leading comments in nested block mappings (168c0e3)
- Handle tags and quoted scalars inline after document start marker (620bdd9)
- Fix explicit key and empty key loader bugs (a8dff3b)
- Fix anchor/alias loader bugs for conformance cases (9552340)
- Eliminate all loader conformance failures (d4b6602)
- Remove formatter workaround and fix all remaining conformance failures (96c0a57)

## Documentation

- Add architecture reference (3955b2a)
- Add feature-log.md for rlsp-yaml-parser and rlsp-fmt (5aa4b8b)
- Cross-link docs from crate READMEs (a806dca)

## Features

- Add CollectionStyle to AST mapping and sequence nodes (728d182)
- Surface document marker flags in AST (4740d10)
- Add interacting-settings fixtures, wire bracket_spacing, and update docs (1c2e974)

## Refactoring

- Remove 7 duplicate tests from conformance.rs (09d2550)
- Move quoted-key tests to correct homes (59715f3)
- Remove duplicate DoS-limit tests from loader.rs (95c0b5b)
- Restructure conformance tests into module (3ca38ea)

## Features

- Add block-sequence plain scalar fast path (05d21fa)

## Refactoring

- Parameterize plain.rs tests with rstest (96f8df6)
- Parameterize quoted.rs tests with rstest (d563134)
- Parameterize block.rs tests with rstest (451a69a)
- Parameterize comment.rs and lines.rs tests with rstest (baa2ee5)
- Parameterize chars/encoding/pos/lexer tests with rstest (5e80a5d)
- Parameterize integration tests with rstest (c8437c1)
- Convert smoke.rs uniform test groups to rstest parameterized tests (a70cd02)
- Split smoke.rs into per-module test directory (8809c48)
- Promote clippy::panic to workspace-level deny (37e66c0)
- Remove unused load_one test helper (be0f7f3)
- Replace #[allow] with #[expect(reason)] and enforce via workspace lints (b248fca)
- Clean up stale comments, loops, and if/else chain (10be323)

## Bug Fixes

- Use physical line indent for anchor/tag before mapping key (55c3846)
- Decode quoted block mapping keys and attach trailing comments (620720f)
- Correct byte/char conflation in position arithmetic (a96460d)

## Documentation

- Update benchmarks with post-optimization results (7c2ca4c)
- Benchmark results for lazy Pos optimization (f2a8f9b)
- Record byte-level scanning benchmark results (0881d88)
- Add plan for code quality improvements (2bb0537)
- Add Tasks 7b/8b/9b and remove stale source checklist (d3b3524)
- Clean benchmarks.md to current-state snapshot (73c3371)
- Add README for rlsp-yaml-parser crate (cdd1a18)

## Features

- Replace Vec<Token> with SmallVec in combinator Reply (15f84d6)
- Replace PEG parser with streaming implementation (cc5c9a5)
- Validate verbatim tag URIs against YAML 1.2 §6.8.1 (ad790db)

## Performance

- Eliminate O(n²) patterns in validate_tokens (39ba760)
- Lazy Pos tracking — drop char_offset, eliminate per-character walk (ea47bb9)
- Byte-level scanning with memchr for plain scalars (c6c56ba)
- Byte-level scanning with memchr for quoted scalars (815d7c5)
- Byte-level scanning for comments and trailing comments (cf772a9)
- Unify pos_after_line with Eof-safe O(1) fast path (32a2809)
- Replace end-of-span char walks with ASCII-fast-path helper (5966502)
- Dispatch scalar try-chain on first-byte peek (8650780)
- Cache step_in_document trim and short-circuit marker checks (ba11228)
- Reorder step_in_document probes by frequency (4728ea3)

## Refactoring

- Split lexer into submodules (c1ff3ce)
- Remove dead chars.rs predicates and de-duplicate (17abda2)
- Colocate is_directive_or_blank_or_comment with its test (4c9428f)
- Extract loader/stream.rs submodule (2ac29d0)
- Extract loader/reloc.rs submodule (3e1ff8a)
- Extract loader/comments.rs submodule (c835896)
- Extract security-limit constants into limits.rs (769b1dc)
- Extract DirectiveScope into directive_scope.rs (7a7127e)
- Extract state-machine enums into src/state.rs (7b04cd0)
- Extract tag/anchor scanning into src/properties.rs (b171ce1)
- Extract mapping-key helpers into src/mapping.rs (69596e2)
- Consolidate pending_anchor into PendingAnchor enum (c5913f1)
- Consolidate pending_tag into PendingTag enum (5b316fc)
- Fold EventIter::failed into IterState::Done (fd183ab)
- Extract EventIter base methods into event_iter/base.rs (9555145)
- Extract directive parsing into event_iter/directives.rs (56a603a)
- Extract handle_flow_collection into event_iter/flow.rs (d6170c4)
- Extract step_in_document into event_iter/step.rs (d1f0e10)
- Extract block-sequence handlers into event_iter/block/sequence.rs (a7657ab)
- Extract block-mapping handlers into event_iter/block/mapping.rs (8705972)
- Relocate properties module into event_iter/ (4316828)
- Relocate directive_scope module into event_iter/ (01a4f3d)
- Relocate state module into event_iter/ (b66b26a)
- Rename mapping to line_mapping and relocate into event_iter/ (9a48d38)
