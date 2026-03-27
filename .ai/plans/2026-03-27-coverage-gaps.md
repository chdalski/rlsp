**Repository:** root
**Status:** Completed (2026-03-27)
**Created:** 2026-03-27

## Goal

Improve test coverage from 94.2% toward ~97%+ by adding
unit tests for uncovered code paths across all modules
except main.rs. Focus on the highest-gap modules first.

## Context

- Current overall line coverage: 94.16% (623 uncovered / 10672)
- main.rs (0%) excluded — binary entry point
- server.rs (76.9%) is the largest gap but requires
  integration tests against the LSP Backend struct
- Most other gaps are edge cases in pure functions —
  straightforward to test
- All existing tests are in-module `#[cfg(test)]` blocks
- Coverage measured with cargo-llvm-cov

## Steps

- [x] Add tests for server.rs uncovered paths (81da39f, 76.9% → 96.5%)
- [x] Add tests for completion.rs uncovered paths (863cc60)
- [x] Add tests for hover.rs uncovered paths (ad5991e)
- [x] Add tests for validators.rs uncovered paths (ad5991e)
- [x] Add tests for schema.rs uncovered paths (adc42af)
- [x] Add tests for symbols.rs, folding.rs, semantic_tokens.rs, selection.rs (f267ae2)
- [x] Verify coverage improvement (94.16% → 96.52%)

## Tasks

### Task 1: server.rs coverage (83 uncovered lines)

The LSP Backend struct has uncovered initialization,
configuration, and request dispatch paths. These need
integration-style tests that create a Backend instance
and call its LanguageServer trait methods.

Investigate the uncovered lines, then add tests targeting
those paths. Focus on testable wiring — skip paths that
require actual stdio transport.

### Task 2: completion.rs coverage (104 uncovered lines)

Uncovered paths include cursor context detection in
sequences, quote-aware colon finding, schema-driven
default value formatting, and edge cases in key/value
completion. These are pure functions — add unit tests.

### Task 3: hover.rs + validators.rs coverage (137 uncovered lines)

Hover has uncovered formatting edge cases (block scalar
detection, quote-aware splitting). Validators have
uncovered error paths in custom tag validation and key
ordering. Both are pure functions — add unit tests.

### Task 4: schema.rs coverage (64 uncovered lines)

Schema fetching, caching, and error handling paths.
Some involve HTTP (harder to test), others are pure
parsing functions. Focus on the testable parsing/type
resolution paths.

### Task 5: symbols, folding, semantic_tokens, selection (122 uncovered lines)

Smaller gaps across four modules. Mostly edge cases:
multi-document folding, comment folding, sequence item
symbols, selection in edge positions. All pure functions.

## Decisions

- **Skip main.rs** — 6 lines, binary entry point, not
  worth the test infrastructure
- **Task ordering** — server.rs first because it has the
  worst percentage; then by absolute gap size
- **No mocking** — pure functions don't need mocks; server
  tests use the Backend struct directly
