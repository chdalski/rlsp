# Changelog


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
