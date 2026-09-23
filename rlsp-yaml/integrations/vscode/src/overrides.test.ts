import { readFileSync } from 'fs';
import * as path from 'path';
import { describe, expect, it } from 'vitest';

// Regression guard for the pnpm.overrides entries that patch npm security
// advisories. brace-expansion is covered by GHSA-rgw5-rvv9-x895 (HIGH,
// CVE-2026-69152: DoS via unbounded intermediate arrays, bypassing the
// earlier GHSA-mh99-v99m-4gvg / CVE-2026-14257 mitigation -- which is why
// the major-5 floor below is 5.0.9, not that earlier advisory's 5.0.8).
// Verified directly against the GitHub Advisory API. For the record, the
// full set of vulnerable bands and patched floors the advisory publishes:
// `< 1.1.18` (patched 1.1.18), `>= 2.0.0, < 2.1.4` (patched 2.1.4),
// `>= 3.0.0, < 3.0.6` (patched 3.0.6), and `>= 4.0.0, < 5.0.9` (patched
// 5.0.9). Major 4 has no patched release at all -- every 4.x version is
// vulnerable, and the fix is moving to major 5. This is reference data
// only, not executable acceptance -- see isPatchedBraceExpansion below for
// which of these floors this guard actually checks against.
//
// Both brace-expansion majors reachable in this graph carry active
// overrides, and both are load-bearing. The test for whether an override is
// redundant: every direct dependent's own declared range for the package
// must be bounded at or above that major line's patched floor. A scratch
// dependency resolution landing above the floor does NOT establish this --
// pnpm picks the newest version satisfying a range when nothing else
// constrains it, so an observed resolved version reflects resolver
// preference (registry mirror state, store contents, incremental lockfile
// history), not a guarantee. Applying the declared-range test here:
//   - minimatch@10.2.6 declares "brace-expansion": "^5.0.8", which admits
//     the vulnerable 5.0.8 -- reaches the extension's runtime dependency
//     path via vscode-languageclient.
//   - minimatch@9.0.9 declares "brace-expansion": "^2.0.2", which admits
//     the vulnerable 2.0.2-2.1.3 range -- reaches the graph via mocha, a
//     dev-only path.
// Neither declared range is bounded at its floor, so neither override is
// redundant. The brace-expansion@2 override was previously retired on the
// resolution-snapshot test above (an earlier audit saw a resolved version
// above the floor and concluded the dependent's range must be safe), which
// is why it is restored here rather than left absent: the retirement was
// based on the wrong test, not on a change in the actual dependency graph.
//
// isPatchedBraceExpansion() below is the single decision point for "is this
// brace-expansion version patched". It dispatches on major line rather than
// comparing against one global floor, because a major-agnostic comparison
// against, say, the 5.0.9 floor would accept any higher major
// (isAtLeast('6.0.0', '5.0.9') is true) without that major ever being
// vetted against the advisory. It encodes a floor only for the two major
// lines actually reachable in this graph today -- 2 (2.1.4) and 5 (5.0.9)
// -- and fails closed on every other major, including 1 and 3, even though
// the advisory gives those their own patched floors (1.1.18 and 3.0.6
// respectively, see above). This mirrors isPatchedFastUri immediately
// below, which fails closed on fast-uri's 2.x line even though 2.4.5 is
// also patched: a transitive jump onto a major line nobody has vetted for
// this graph is unexpected enough that it should stop the build and be
// looked at, rather than pass on the assumption that the advisory's own
// floor is automatically safe to trust unattended. Major 1 and major 3
// being absent from this graph is not an oversight -- it is why their
// floors are not wired in as passing thresholds. Major 4 fails closed for
// a different reason: the advisory covers it as fully vulnerable, with no
// patched release to check against at all.
//
// The lockfile-driven "resolved major lines are exactly {2, 5}" test below
// is an independent structural canary, not a restatement of the predicate:
// even if a future edit broadened the predicate's accepted majors, that
// test still fails loudly the moment an unexpected major line appears in
// the graph, which is the actual property this guard exists to protect.
//
// fast-uri carries no override -- its version drifts with ordinary
// transitive dependency updates, and as of the @vscode/vsce 4.0.0 upgrade
// it has left the dependency graph entirely (vsce 4.0.0 dropped the
// dependency chain that pulled it in). isPatchedFastUri() encodes its own
// verified patched floors (3.1.6 and 4.1.3, from GHSA-5jgf-p345-68v8 /
// GHSA-fph4-wmhf-6fwf / GHSA-f65p-4m7j-42xc / GHSA-jqff-g426-hqxp) and
// fails closed for every other major line, including 2.x, even though
// 2.4.5 is also patched: a transitive downgrade across a major line is
// unexpected enough that it should stop and be looked at rather than pass
// on an assumption.
//
// All of this asserts what the lockfile actually resolves -- not just that
// an override string is present in package.json.
const lockfilePath = path.join(__dirname, '..', 'pnpm-lock.yaml');

// Windows checkouts of this repository read text files with CRLF line
// endings unless normalized at checkout (see ../.gitattributes, which
// pins pnpm-lock.yaml to LF as the checkout-time fix). Normalizing here
// too makes the parsing below robust for any checkout that predates that
// pin, or has a stale local git config. This only changes how this test
// process reads the file into memory -- it does not rewrite the file on
// disk and does not change what pnpm itself uses to resolve or install
// packages.
function normalizeLineEndings(text: string): string {
  return text.replace(/\r\n/g, '\n');
}

const lockfile = normalizeLineEndings(readFileSync(lockfilePath, 'utf8'));

// Slice the `overrides:` block out of a lockfile string.
function overridesBlockOf(text: string): string {
  const overridesStart = text.indexOf('overrides:');
  const overridesEnd = text.indexOf('\n\n', overridesStart);
  return text.slice(overridesStart, overridesEnd === -1 ? undefined : overridesEnd);
}

// Every `<packageName>@X.Y.Z:` header in the lockfile, deduplicated. Used to
// check a package across every resolved version simultaneously present in
// the graph, not just the one an override would have forced.
function allResolvedVersions(text: string, packageName: string): string[] {
  const headerPattern = new RegExp(`\\n  ${packageName}@(\\d+\\.\\d+\\.\\d+):`, 'g');
  const versions = new Set<string>();
  for (const match of text.matchAll(headerPattern)) {
    const version = match[1];
    // The capture group is unconditional in the pattern, so it is always
    // present whenever the overall match succeeds -- this branch is
    // unreachable in practice. It throws rather than skipping the match
    // because silently dropping a resolved version here would shrink the
    // set this security guard checks, which is worse than a crash.
    if (version === undefined) {
      throw new Error(`unexpected header match with no captured version: ${match[0]}`);
    }
    versions.add(version);
  }
  return [...versions];
}

// Plain `major.minor.patch` version comparison. The lockfile only ever
// carries release versions for the packages this guard checks, so an
// unexpected format (pre-release tag, build metadata) is a hard failure
// rather than something to silently miscompare.
function parseVersion(version: string): [number, number, number] {
  const match = /^(\d+)\.(\d+)\.(\d+)$/.exec(version);
  if (!match) {
    throw new Error(`unexpected version format: ${version}`);
  }
  return [Number(match[1]), Number(match[2]), Number(match[3])];
}

function isAtLeast(actual: string, floor: string): boolean {
  const [aMajor, aMinor, aPatch] = parseVersion(actual);
  const [fMajor, fMinor, fPatch] = parseVersion(floor);
  if (aMajor !== fMajor) return aMajor > fMajor;
  if (aMinor !== fMinor) return aMinor > fMinor;
  return aPatch >= fPatch;
}

// fast-uri-specific patched-version check. isAtLeast alone cannot express
// this: it is major-agnostic, so isAtLeast(version, '3.1.6') accepts any
// 4.x version, including the vulnerable 4.0.0-4.1.2 range. This function
// is the single decision point for "is this fast-uri version patched" --
// keeping the floor check and the major check in one place means there is
// no way to satisfy the guard by editing only one of them.
function isPatchedFastUri(version: string): boolean {
  const [major] = parseVersion(version);
  if (major === 3) return isAtLeast(version, '3.1.6');
  if (major === 4) return isAtLeast(version, '4.1.3');
  return false; // any other major line is unvetted -- fail closed
}

// brace-expansion-specific patched-version check for GHSA-rgw5-rvv9-x895.
// Only majors 2 and 5 -- the two lines actually reachable in this graph --
// get a floor comparison. Every other major fails closed, including 1 and
// 3, which the advisory covers with patched floors of their own (1.1.18
// and 3.0.6, see header comment): those floors are deliberately not wired
// in here, because a transitive jump onto a major line nobody has vetted
// for this graph should stop the build, not pass because the advisory
// happens to have a floor for it. Major 4 falls into the same `false`
// default for a different reason -- it has no patched release at all, so
// there is no floor to check even if this guard wanted to vet it.
function isPatchedBraceExpansion(version: string): boolean {
  const [major] = parseVersion(version);
  if (major === 2) return isAtLeast(version, '2.1.4');
  if (major === 5) return isAtLeast(version, '5.0.9');
  return false; // majors 1, 3, 4, and anything else are unvetted -- fail closed
}

// Aggregation predicate for an overridden brace-expansion major line: every
// resolved version on that line must be patched, AND the line must be
// non-empty. The presence check is not redundant -- `[].every(...)` is
// vacuously true, so without it, an override silently going dead (its major
// line vanishing from the graph entirely) would pass. That is the opposite
// of what this guard should do while the override is still active in
// pnpm.overrides (see header comment) -- true today for both the major-2
// and major-5 branches. Deliberately asymmetric with allPatchedOrAbsent
// below, which must accept an empty array: brace-expansion's overridden
// lines are expected to always resolve; fast-uri has no override and is
// not.
function allPatchedAndPresent(versions: string[]): boolean {
  return versions.length > 0 && versions.every((version) => isPatchedBraceExpansion(version));
}

// Aggregation predicate for fast-uri: absence from the graph is
// tolerated (see header comment), but any version that is present must
// be patched. `[].every(…)` being vacuously true is exactly the wanted
// behavior here, unlike allPatchedAndPresent above -- no separate
// presence check.
function allPatchedOrAbsent(versions: string[]): boolean {
  return versions.every((version) => isPatchedFastUri(version));
}

describe('pnpm.overrides regression guard (brace-expansion / fast-uri)', () => {
  it('overrides block declares the retained pins', () => {
    const overridesBlock = overridesBlockOf(lockfile);
    expect(overridesBlock).toContain('brace-expansion@2: ^2.1.4');
    expect(overridesBlock).toContain('brace-expansion@5: ^5.0.9');
    expect(overridesBlock).toContain('serialize-javascript: ^7.0.5');
  });

  it('overrides block no longer declares the removed fast-uri pin', () => {
    const overridesBlock = overridesBlockOf(lockfile);
    expect(overridesBlock).not.toContain('fast-uri:');
  });

  // Structural canary, independent of isPatchedBraceExpansion: even if a
  // future edit broadened the predicate's accepted majors, this still
  // fails the moment an unexpected major line -- one this guard has never
  // vetted for this graph -- shows up in the lockfile at all.
  it('brace-expansion resolves only the vetted major lines (2 and 5)', () => {
    const majors = new Set(
      allResolvedVersions(lockfile, 'brace-expansion').map((version) => parseVersion(version)[0]),
    );
    expect(majors).toEqual(new Set([2, 5]));
  });

  it("brace-expansion's overridden major-2 branch resolves to at least the audited patched floor", () => {
    const majorTwoVersions = allResolvedVersions(lockfile, 'brace-expansion').filter(
      (version) => parseVersion(version)[0] === 2,
    );
    expect(allPatchedAndPresent(majorTwoVersions)).toBe(true);
  });

  it("brace-expansion's overridden major-5 branch resolves to at least the audited patched floor", () => {
    const majorFiveVersions = allResolvedVersions(lockfile, 'brace-expansion').filter(
      (version) => parseVersion(version)[0] === 5,
    );
    expect(allPatchedAndPresent(majorFiveVersions)).toBe(true);
  });

  it('fast-uri is absent from the lockfile, or resolves to a patched version wherever present', () => {
    const versions = allResolvedVersions(lockfile, 'fast-uri');
    expect(allPatchedOrAbsent(versions)).toBe(true);
  });
});

// Direct unit coverage for isAtLeast/parseVersion. The lockfile-driven
// tests above only ever exercise these against whatever fast-uri/
// brace-expansion versions happen to be resolved right now -- once the
// lockfile holds a non-vulnerable version, no floor value in those tests
// can exercise the false branch again. These cases are the standing proof
// that the comparison itself rejects vulnerable versions, independent of
// what the lockfile currently resolves.
describe('isAtLeast', () => {
  it.each([
    // The exact regression this task closes: the pre-fix floor accepted
    // this vulnerable patch version.
    ['3.1.5', '3.1.6', false],
    // Inclusive floor.
    ['3.1.6', '3.1.6', true],
    // Currently resolved by the lockfile.
    ['3.1.7', '3.1.6', true],
    // Numeric comparison, not lexicographic -- '3.1.10' < '3.1.6' as
    // strings, but 10 > 6 as numbers.
    ['3.1.10', '3.1.6', true],
    ['3.2.0', '3.1.6', true],
    ['4.0.0', '3.1.6', true],
    ['3.0.9', '3.1.6', false],
    ['2.9.9', '3.1.6', false],
    // The brace-expansion@5 patched floor (GHSA-rgw5-rvv9-x895): the
    // override's caret range (^5.0.9) permits ordinary patch drift above
    // this floor, but allPatchedAndPresent below must still catch drift
    // below it.
    ['5.0.8', '5.0.9', false],
    ['5.0.9', '5.0.9', true],
  ])('isAtLeast(%s, %s) === %s', (actual, floor, expected) => {
    expect(isAtLeast(actual, floor)).toBe(expected);
  });
});

// Boundary coverage for isPatchedFastUri, independent of what the
// lockfile currently resolves. Covers both patched floors, the two 4.x
// false-positive traps (4.0.0 is major-4 but pre-floor; 4.1.2 is one
// patch below the floor) that isAtLeast alone would miss, and the
// fail-closed default for majors this guard has not vetted.
describe('isPatchedFastUri', () => {
  it.each([
    ['3.1.5', false],
    ['3.1.6', true],
    ['4.0.0', false],
    ['4.1.2', false],
    ['4.1.3', true],
    ['4.2.0', true],
    ['2.9.9', false],
    ['5.0.0', false],
  ])('isPatchedFastUri(%s) === %s', (version, expected) => {
    expect(isPatchedFastUri(version)).toBe(expected);
  });
});

// Boundary coverage for isPatchedBraceExpansion, independent of what the
// lockfile currently resolves. Covers both vetted floors (2 and 5), the
// major-4 line (which has no patched release at all, so every version on
// it is false regardless of how high the minor/patch climbs), and the
// fail-closed default for every other major -- including the two trap
// cases below, which are the load-bearing proof that this predicate
// rejects unvetted majors even when the advisory gives them their own
// patched floor. Without those two cases, a predicate that (incorrectly)
// fell back to the advisory's full floor table for majors 1 and 3 would
// still pass every other case in this table.
describe('isPatchedBraceExpansion', () => {
  it.each([
    // Trap: this is major 1's real advisory-patched floor, but major 1 has
    // no dependent in this graph, so it must stay unvetted -- same
    // reasoning as isPatchedFastUri's major-2 case above.
    ['1.1.18', false],
    // The exact regression this task restores the override for: the top
    // of minimatch@9.0.9's admitted range (^2.0.2) sits inside this
    // vulnerable band.
    ['2.1.3', false],
    ['2.1.4', true],
    ['2.5.0', true],
    // Trap: major 3's real advisory-patched floor -- must still reject,
    // for the same reason as 1.1.18 above.
    ['3.0.6', false],
    // Major 4's lowest version -- no patched release exists on this line.
    ['4.0.0', false],
    // High into major 4, still vulnerable -- proves this isn't
    // accidentally implemented as a floor check (e.g. isAtLeast(v,
    // '4.0.0'), which would wrongly return true here).
    ['4.9.9', false],
    ['5.0.8', false],
    ['5.0.9', true],
    ['5.1.0', true],
    // Fail-closed default: major 0 also falls inside the advisory's real
    // `< 1.1.18` band, so this is doubly correct, not just unvetted.
    ['0.9.9', false],
    // Fail-closed default for a future major the advisory doesn't cover
    // at all.
    ['6.0.0', false],
  ])('isPatchedBraceExpansion(%s) === %s', (version, expected) => {
    expect(isPatchedBraceExpansion(version)).toBe(expected);
  });
});

// Boundary coverage for allPatchedAndPresent's aggregation semantics,
// independent of what the lockfile currently resolves. isPatchedBraceExpansion's
// own floor/major comparisons are already covered above; these cases are
// scoped to the aggregation behavior only (empty-array, mixed-major array).
describe('allPatchedAndPresent', () => {
  it.each([
    [['2.1.4'], true],
    [['2.1.5'], true],
    // The exact regression this restores the override for.
    [['2.1.3'], false],
    // Inclusive floor, single element -- the shape of today's real
    // lockfile major-5 branch.
    [['5.0.9'], true],
    [['5.0.8'], false],
    // Mixed majors, both patched -- proves the aggregator honors each
    // element's own major-dispatched floor rather than one global floor,
    // which could not express two simultaneously-valid floors at once.
    [['2.1.4', '5.0.9'], true],
    // Mixed majors, one bad -- proves every element is checked, not just
    // the first.
    [['2.1.4', '5.0.8'], false],
    // A major-4 version on its own -- proves the aggregator doesn't
    // bypass isPatchedBraceExpansion's always-false major-4 handling.
    [['4.0.0'], false],
    // Empty must fail: both overrides are active (see header comment on
    // overrides.test.ts), so a vanished branch is itself a regression,
    // not a pass.
    [[], false],
  ])('allPatchedAndPresent(%j) === %s', (versions, expected) => {
    expect(allPatchedAndPresent(versions)).toBe(expected);
  });
});

// Boundary coverage for allPatchedOrAbsent's aggregation semantics,
// independent of what the lockfile currently resolves. isPatchedFastUri's
// own floor/major comparisons are already covered above; these cases are
// scoped to the aggregation behavior only (empty-array, mixed-array).
describe('allPatchedOrAbsent', () => {
  it.each([
    // Empty must pass -- the tolerated-absence behavior this task adds.
    [[], true],
    [['3.1.6'], true],
    [['4.1.3'], true],
    // Present and unpatched must still fail.
    [['3.1.5'], false],
    // Mixed patched+unpatched -- proves no short-circuit on the first
    // good value.
    [['4.1.3', '3.1.5'], false],
    // Present but on an unvetted major line -- confirms this wrapper
    // doesn't bypass isPatchedFastUri's fail-closed default.
    [['5.0.0'], false],
  ])('allPatchedOrAbsent(%j) === %s', (versions, expected) => {
    expect(allPatchedOrAbsent(versions)).toBe(expected);
  });
});

describe('parseVersion', () => {
  it('throws on a non-release version string', () => {
    expect(() => parseVersion('3.1.6-beta.1')).toThrow('unexpected version format');
  });
});

describe('normalizeLineEndings', () => {
  it('converts CRLF to LF without corrupting content', () => {
    const input = 'a:\r\n  b: 1\r\n\r\nc:\r\n  d: 2\r\n';
    const expected = 'a:\n  b: 1\n\nc:\n  d: 2\n';
    expect(normalizeLineEndings(input)).toBe(expected);
  });

  it('is a no-op on already-LF content', () => {
    const input =
      'packages:\n\n  minimatch@5.1.9:\n    dependencies:\n      brace-expansion: 2.1.4\n';
    expect(normalizeLineEndings(input)).toBe(input);
  });
});
