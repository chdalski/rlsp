**Repository:** root
**Status:** NotStarted
**Created:** 2026-09-23

## Goal

`rlsp-yaml/integrations/vscode/src/overrides.test.ts` has grown to 415 lines
across eight plans, each adding a predicate, a boundary table, or another
paragraph of header prose, and no plan has ever been scoped to look at the
aggregate. It now carries two structurally identical patched-version
predicates, two aggregation predicates that differ only in how they treat an
empty set, and roughly 75 lines of comment before the first line of code.
Collapse the duplicated pairs into one parameterised form each, cut the
header to the reasoning that is still load-bearing, and order both guard
files so each helper sits with the suite that proves it. Behavior does not
change: the same versions are accepted and rejected after this work as
before it.

## Context

- **Key files.** `rlsp-yaml/integrations/vscode/src/overrides.test.ts` (415
  lines) and `rlsp-yaml/integrations/vscode/src/engine-compat.test.ts` (264
  lines). Neither has a production counterpart module — each test file *is*
  the guard it implements, which is a deliberate design recorded in both
  files.

- **What is duplicated inside `overrides.test.ts`.**
  - `isPatchedFastUri` (floors for majors 3 and 4) and
    `isPatchedBraceExpansion` (floors for majors 2 and 5) have identical
    shape: dispatch on major line, compare against that line's floor,
    return false for every other major.
  - `allPatchedAndPresent` and `allPatchedOrAbsent` differ only in whether
    an empty array passes. That difference is deliberate and must survive:
    `brace-expansion`'s overridden lines are expected always to resolve, so
    an empty set means an override went dead and must fail; `fast-uri`
    carries no override and has left the graph entirely, so an empty set
    must pass.

- **What is duplicated between the two files, and stays.**
  `normalizeLineEndings` is copied into both. The copy is deliberate and
  both files carry a comment justifying it — there is no precedent for
  importing helpers across test files here, and a shared module would
  change the "the test file is the guard" design. `parseVersion` exists in
  both files but returns `[major, minor, patch]` in one and
  `[major, minor]` in the other; they are different functions, not copies.

- **`engine-compat.test.ts` has no duplication problem of its own.** Its
  header is 13 lines, every helper it declares is used, and its structure is
  sound. Its slice of this work is ordering only.

- **The header comment's load-bearing content.** Three things in it are the
  reason earlier work went wrong and must survive any trim: the
  declared-range test for whether an override is redundant; the record that
  the `brace-expansion@2` override was retired on the wrong test and
  restored; and the fail-closed rationale for major lines the graph does not
  reach. The advisory's full band table is reference data, also worth
  keeping, but stated once rather than restated per predicate.

- **The same fail-closed rationale is currently written three times** — once
  for `brace-expansion`, once again for its majors 1 and 3, and once for
  `fast-uri`. One statement covers all three cases.

- **Behavior parity is mechanically checkable.** The suite is 117 tests
  across 6 files at baseline, of which `overrides.test.ts` and
  `engine-compat.test.ts` contribute the guard cases. Every predicate and
  aggregator already has a literal `it.each` boundary table, so a
  behavior-preserving refactor keeps every one of those cases passing with
  the same expected values.

- **Build and test commands** are in the root `CLAUDE.md` under Build and
  Test. `pnpm run test:integration` needs a display — `xvfb-run -a` on
  Linux.

## Steps

- [x] Survey both guard files and identify what is genuinely duplicated
- [x] Confirm scope and approach with the user
- [ ] Collapse the duplicated predicates and aggregators
- [ ] Cut the header comment to its load-bearing reasoning
- [ ] Order both files so each helper sits with the suite that proves it

## Tasks

### Task 1: One patched-version predicate and one aggregator replace the duplicated pairs

Replace `isPatchedFastUri` and `isPatchedBraceExpansion` with a single
predicate driven by a per-package floor table, and replace
`allPatchedAndPresent` and `allPatchedOrAbsent` with a single aggregator
that takes the empty-set policy as an argument. Every version accepted
before is accepted after, and every version rejected before is rejected
after. The header comment's prose is rewritten wholesale by the task that
follows this one, so leaving its references to the merged function names in
place here is expected rather than an oversight.

- [ ] One predicate decides whether a version of a named package is
      patched, reading that package's per-major floors from a single
      declared table, and returning false for any major line the table does
      not cover
- [ ] The floors the table declares are exactly those in force today:
      `brace-expansion` majors 2 and 5 at 2.1.4 and 5.0.9, `fast-uri`
      majors 3 and 4 at 3.1.6 and 4.1.3. No major line gains or loses a
      floor
- [ ] One aggregator decides whether a set of resolved versions passes,
      with the empty-set outcome supplied by its caller. The
      `brace-expansion` call sites reject an empty set and the `fast-uri`
      call site accepts one, as they do today
- [ ] Every boundary case asserted against the old predicates and
      aggregators is asserted against the new ones, with the same expected
      value. This includes the advisory-floor traps 1.1.18 and 3.0.6, the
      major-4 cases, the uncovered-major case, and the empty-array cases
      for both aggregation policies
- [ ] No test is deleted without an equivalent assertion existing
      afterwards, and the suite's passing count is at least its count at
      baseline
- [ ] `pnpm run test`, `pnpm run typecheck`, `pnpm run lint`, and
      `pnpm run format` pass
- [ ] `pnpm run audit` reports no finding beyond the allowlisted low

### Task 2: The header comment states each piece of reasoning once

Cut `overrides.test.ts`'s header to the reasoning a future maintainer needs,
stating the fail-closed rationale once instead of three times. The material
that explains why earlier work went wrong stays.

- [ ] The header states the declared-range test for override redundancy,
      the record that the `brace-expansion@2` override was retired on the
      wrong test and restored, the fail-closed rationale for unvetted major
      lines, and the advisory's vulnerable bands with their patched floors
- [ ] The fail-closed rationale appears once and covers every package and
      major line it applies to, rather than being restated per predicate
- [ ] No statement in the header contradicts what the file asserts, and no
      statement describes a package's override status or resolved version as
      a fact about the current moment
- [ ] `pnpm run test`, `pnpm run typecheck`, `pnpm run lint`, and
      `pnpm run format` pass

### Task 3: Each helper sits with the suite that proves it

Order both guard files so a reader meets a helper and its boundary suite
together, rather than reading every helper and then every suite. No helper,
assertion, or test case changes.

- [ ] In both files, each helper and the suite asserting its behavior are
      adjacent, with the lockfile-driven and manifest-driven suites that
      exercise the real files grouped together
- [ ] Every helper, assertion, test name and expected value is unchanged
      from before this task — the diff moves lines and changes comments,
      and changes nothing else
- [ ] The suite's passing count equals its count before this task
- [ ] `pnpm run test`, `pnpm run typecheck`, `pnpm run lint`, and
      `pnpm run format` pass
- [ ] `pnpm run build` and `pnpm run test:integration` pass

## Decisions

- **Consolidation happens inside each file; no shared helper module is
  introduced** (user choice). Both guard files record that the test file is
  the guard, with no production counterpart. A shared module would change
  that design and would need its own tests.

- **The cross-file duplication of `normalizeLineEndings` stays** (follows
  from the decision above). It is the only true copy between the files, and
  removing it requires the shared module that was declined. The comments
  justifying the copy remain accurate.

- **`engine-compat.test.ts` is in scope for ordering only** (user choice to
  include both files). It has no duplicated helpers of its own and a
  13-line header, so the other two tasks do not apply to it.

- **The empty-set asymmetry is preserved, not unified.** It is the property
  that makes a dead override fail loudly, and collapsing it would silently
  weaken the guard.

## Non-Goals

- Extracting any helper into a production module or a shared test utility.
- Changing which versions the guards accept or reject.
- Changing `pnpm.overrides`, `pnpm.auditConfig`, or any dependency.
- Consolidating `commands.test.ts`, `config.test.ts`, `server.test.ts` or
  `status.test.ts`. They have not shown this growth pattern.
- Trimming `engine-compat.test.ts`'s header, which is already short.
