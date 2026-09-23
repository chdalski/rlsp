import { readFileSync } from 'fs';
import * as path from 'path';
import { describe, expect, it } from 'vitest';

// Regression guard for the pnpm.overrides entries that patch npm security
// advisories. brace-expansion is covered by GHSA-rgw5-rvv9-x895 (HIGH,
// CVE-2026-69152: DoS via unbounded intermediate arrays, bypassing the
// earlier GHSA-mh99-v99m-4gvg / CVE-2026-14257 mitigation -- which is why
// the major-5 floor below is 5.0.9, not that earlier advisory's 5.0.8).
// Verified directly against the GitHub Advisory API. The full set of
// vulnerable bands and patched floors GHSA-rgw5-rvv9-x895 publishes:
// `< 1.1.18` (patched 1.1.18), `>= 2.0.0, < 2.1.4` (patched 2.1.4),
// `>= 3.0.0, < 3.0.6` (patched 3.0.6), and `>= 4.0.0, < 5.0.9` (patched
// 5.0.9). Major 4 has no patched release at all -- every 4.x version is
// vulnerable and the fix is moving to major 5, so a missing major-4 floor
// below means migrating off major 4 entirely, not that no floor has been
// found yet. fast-uri is covered by GHSA-5jgf-p345-68v8 /
// GHSA-fph4-wmhf-6fwf / GHSA-f65p-4m7j-42xc / GHSA-jqff-g426-hqxp, with
// verified patched floors of 3.1.6 and 4.1.3. All of the above is
// reference data only, not executable acceptance -- see the patchedFloors
// table and isPatchedVersion below for which of these floors this guard
// actually checks against.
//
// Before retiring either override: re-derive the current direct
// dependents from the lockfile and check each one's own declared range
// against the floor below -- a resolved version is not evidence. An
// override is redundant only when every direct dependent's declared
// range is bounded at or above that major line's patched floor; a
// resolved version above the floor proves nothing, because pnpm picks
// the newest version satisfying a range when nothing else constrains it,
// so an observed resolved version reflects resolver preference (registry
// mirror state, store contents, incremental lockfile history), not a
// range guarantee. The brace-expansion@2 override was once retired on
// exactly that wrong evidence -- a resolved version above the floor was
// read as proof the dependent's range was safe -- and was restored once
// the retirement was found to rest on a resolved version rather than a
// bounded declared range. Re-derive from the lockfile; don't trust a
// snapshot.
//
// The patchedFloors table (checked by isPatchedVersion below) only
// contains the major lines vetted as reachable in this graph for each
// package, not every major either advisory has patched -- for example
// brace-expansion majors 1 and 3, and fast-uri major 2, each covered by
// their own advisory floor but with no entry in that table. A major
// absent from a package's row is unvetted, not safe, and the lookup
// fails closed on it rather than falling back to another major or
// another package; see patchedFloors/isPatchedVersion below for the
// exact mechanics. The resolved-major-lines canary test below
// independently fails the moment an unvetted brace-expansion major
// appears in the lockfile at all; fast-uri has no equivalent canary, so
// the fail-closed lookup is its only backstop.
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

// Package-specific patched-version check, keyed first by package name and
// then by major line. isAtLeast alone cannot express this: it is
// major-agnostic, so isAtLeast(version, floor) for one major's floor would
// accept any higher major, including majors nobody has vetted for this
// graph. Keying the floor table by package name first -- rather than one
// shared major -> floor map -- prevents one package's vetted floor for a
// major line from leaking into another package's fail-closed default for
// that same major: fast-uri's major-3/4 floors and brace-expansion's
// major-2/5 floors never collide, because each package only ever looks up
// its own row. The GuardedPackage union restricts the package parameter to
// the two packages this guard vets, so a typo in the package name is a
// compile error rather than a silent miss.
type GuardedPackage = 'brace-expansion' | 'fast-uri';

// The single declared table of patched floors. brace-expansion majors 2
// and 5 are the two lines reachable in this graph (GHSA-rgw5-rvv9-x895;
// see header comment for the advisory's full floor list, most of which is
// deliberately not wired in here). fast-uri majors 3 and 4 are its two
// verified patched floors (GHSA-5jgf-p345-68v8 / GHSA-fph4-wmhf-6fwf /
// GHSA-f65p-4m7j-42xc / GHSA-jqff-g426-hqxp). Any major line absent from a
// package's row -- including majors the advisories cover with floors of
// their own, like brace-expansion 1 and 3, or fast-uri 2 -- has no entry,
// and isPatchedVersion below fails closed on a missing entry rather than
// falling back to any other row or major.
const patchedFloors: Record<GuardedPackage, Record<number, string>> = {
  'brace-expansion': { 2: '2.1.4', 5: '5.0.9' },
  'fast-uri': { 3: '3.1.6', 4: '4.1.3' },
};

// Single decision point for "is this version of this package patched".
// Looks up the package's row, then that row's floor for the version's
// major line; a missing entry (an unvetted major for that package) returns
// false rather than falling through to another major's or another
// package's floor. Keeping the floor lookup and the major dispatch in one
// place means there is no way to satisfy the guard by editing only one of
// them.
function isPatchedVersion(packageName: GuardedPackage, version: string): boolean {
  const [major] = parseVersion(version);
  const floor = patchedFloors[packageName][major];
  return floor !== undefined && isAtLeast(version, floor);
}

// Single aggregation decision point: every resolved version of the named
// package must be patched, and the empty-set outcome is supplied by the
// caller via the named onEmpty field rather than hardcoded per package --
// a named field keeps the polarity legible at each call site, where a bare
// positional boolean would not be. brace-expansion's call sites pass
// onEmpty: false, because its overridden major lines are expected always
// to resolve -- an empty set means an override silently went dead, which
// must fail loudly rather than pass vacuously on `[].every(...)`. The
// fast-uri call site passes onEmpty: true, because fast-uri carries no
// override and its absence from the graph (true as of the @vscode/vsce
// 4.0.0 upgrade) is tolerated. See header comment for the full reasoning
// behind each policy.
function allPatched(
  versions: string[],
  packageName: GuardedPackage,
  { onEmpty }: { onEmpty: boolean },
): boolean {
  if (versions.length === 0) return onEmpty;
  return versions.every((version) => isPatchedVersion(packageName, version));
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

  // Structural canary, independent of isPatchedVersion: even if a
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
    expect(allPatched(majorTwoVersions, 'brace-expansion', { onEmpty: false })).toBe(true);
  });

  it("brace-expansion's overridden major-5 branch resolves to at least the audited patched floor", () => {
    const majorFiveVersions = allResolvedVersions(lockfile, 'brace-expansion').filter(
      (version) => parseVersion(version)[0] === 5,
    );
    expect(allPatched(majorFiveVersions, 'brace-expansion', { onEmpty: false })).toBe(true);
  });

  it('fast-uri is absent from the lockfile, or resolves to a patched version wherever present', () => {
    const versions = allResolvedVersions(lockfile, 'fast-uri');
    expect(allPatched(versions, 'fast-uri', { onEmpty: true })).toBe(true);
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
    // this floor, but allPatched below must still catch drift
    // below it.
    ['5.0.8', '5.0.9', false],
    ['5.0.9', '5.0.9', true],
  ])('isAtLeast(%s, %s) === %s', (actual, floor, expected) => {
    expect(isAtLeast(actual, floor)).toBe(expected);
  });
});

// Boundary coverage for isPatchedVersion, independent of what the lockfile
// currently resolves. Union of both packages' patched floors and 4.x
// false-positive traps, the fail-closed default for majors this guard has
// not vetted for each package, and -- new for the merge -- the
// cross-contamination traps at the end: brace-expansion 3.1.6/4.1.3 are
// fast-uri's real floors, and fast-uri 2.1.4/5.0.9 are brace-expansion's
// real floors, so a table keyed by major only (dropping packageName) would
// wrongly flip every one of those four cases from false to true. There is
// no runtime "unknown package name" case here: GuardedPackage's literal
// union type makes that a compile error, not a reachable branch.
describe('isPatchedVersion', () => {
  it.each<[GuardedPackage, string, boolean]>([
    // fast-uri, carried forward unchanged from the former
    // isPatchedFastUri table. Covers both patched floors and the two 4.x
    // false-positive traps (4.0.0 is major-4 but pre-floor; 4.1.2 is one
    // patch below the floor) that isAtLeast alone would miss.
    ['fast-uri', '3.1.5', false],
    ['fast-uri', '3.1.6', true],
    ['fast-uri', '4.0.0', false],
    ['fast-uri', '4.1.2', false],
    ['fast-uri', '4.1.3', true],
    ['fast-uri', '4.2.0', true],
    ['fast-uri', '2.9.9', false],
    ['fast-uri', '5.0.0', false],
    // brace-expansion, carried forward unchanged from the former
    // isPatchedBraceExpansion table.
    // Trap: this is major 1's real advisory-patched floor, but major 1 has
    // no dependent in this graph, so it must stay unvetted -- same
    // reasoning as fast-uri's major-2 case above.
    ['brace-expansion', '1.1.18', false],
    // The exact regression this task restores the override for: the top
    // of minimatch@9.0.9's admitted range (^2.0.2) sits inside this
    // vulnerable band.
    ['brace-expansion', '2.1.3', false],
    ['brace-expansion', '2.1.4', true],
    ['brace-expansion', '2.5.0', true],
    // Trap: major 3's real advisory-patched floor -- must still reject,
    // for the same reason as 1.1.18 above.
    ['brace-expansion', '3.0.6', false],
    // Major 4's lowest version -- no patched release exists on this line.
    ['brace-expansion', '4.0.0', false],
    // High into major 4, still vulnerable -- proves this isn't
    // accidentally implemented as a floor check (e.g. isAtLeast(v,
    // '4.0.0'), which would wrongly return true here).
    ['brace-expansion', '4.9.9', false],
    ['brace-expansion', '5.0.8', false],
    ['brace-expansion', '5.0.9', true],
    ['brace-expansion', '5.1.0', true],
    // Fail-closed default: major 0 also falls inside the advisory's real
    // `< 1.1.18` band, so this is doubly correct, not just unvetted.
    ['brace-expansion', '0.9.9', false],
    // Fail-closed default for a future major the advisory doesn't cover
    // at all.
    ['brace-expansion', '6.0.0', false],
    // Cross-contamination traps, new for the merge: fast-uri's real
    // patched floors, applied to brace-expansion, which has no major-3
    // floor at all and a different (fully-vulnerable) major-4 status.
    ['brace-expansion', '3.1.6', false],
    ['brace-expansion', '4.1.3', false],
    // Cross-contamination traps, the other direction: brace-expansion's
    // real patched floors, applied to fast-uri, which vets neither major
    // 2 nor major 5.
    ['fast-uri', '2.1.4', false],
    ['fast-uri', '5.0.9', false],
  ])('isPatchedVersion(%s, %s) === %s', (packageName, version, expected) => {
    expect(isPatchedVersion(packageName, version)).toBe(expected);
  });
});

// Boundary coverage for allPatched's aggregation semantics, independent of
// what the lockfile currently resolves. isPatchedVersion's own floor/major
// comparisons are already covered above; these cases are scoped to the
// aggregation behavior only (empty-array, mixed-major array, and the
// onEmpty flag itself). Union of both former aggregators' tables, plus two
// new traps at the end that pass the *other* package's historical onEmpty
// policy alongside an empty array -- proving onEmpty itself controls the
// outcome rather than a hardcoded per-package branch that happens to match
// today's two call sites.
describe('allPatched', () => {
  it.each<[string[], GuardedPackage, boolean, boolean]>([
    // brace-expansion, carried forward unchanged from the former
    // allPatchedAndPresent table (onEmpty: false throughout).
    [['2.1.4'], 'brace-expansion', false, true],
    [['2.1.5'], 'brace-expansion', false, true],
    // The exact regression this restores the override for.
    [['2.1.3'], 'brace-expansion', false, false],
    // Inclusive floor, single element -- the shape of today's real
    // lockfile major-5 branch.
    [['5.0.9'], 'brace-expansion', false, true],
    [['5.0.8'], 'brace-expansion', false, false],
    // Mixed majors, both patched -- proves the aggregator honors each
    // element's own major-dispatched floor rather than one global floor,
    // which could not express two simultaneously-valid floors at once.
    [['2.1.4', '5.0.9'], 'brace-expansion', false, true],
    // Mixed majors, one bad -- proves every element is checked, not just
    // the first.
    [['2.1.4', '5.0.8'], 'brace-expansion', false, false],
    // A major-4 version on its own -- proves the aggregator doesn't
    // bypass isPatchedVersion's always-false brace-expansion major-4
    // handling.
    [['4.0.0'], 'brace-expansion', false, false],
    // Empty must fail: both overrides are active (see header comment on
    // overrides.test.ts), so a vanished branch is itself a regression,
    // not a pass.
    [[], 'brace-expansion', false, false],
    // fast-uri, carried forward unchanged from the former
    // allPatchedOrAbsent table (onEmpty: true throughout).
    // Empty must pass -- the tolerated-absence behavior fast-uri's call
    // site relies on.
    [[], 'fast-uri', true, true],
    [['3.1.6'], 'fast-uri', true, true],
    [['4.1.3'], 'fast-uri', true, true],
    // Present and unpatched must still fail.
    [['3.1.5'], 'fast-uri', true, false],
    // Mixed patched+unpatched -- proves no short-circuit on the first
    // good value.
    [['4.1.3', '3.1.5'], 'fast-uri', true, false],
    // Present but on an unvetted major line -- confirms this wrapper
    // doesn't bypass isPatchedVersion's fail-closed default.
    [['5.0.0'], 'fast-uri', true, false],
    // onEmpty-flag traps, new for the merge: an empty array with the
    // *other* package's historical policy. If allPatched ignored onEmpty
    // and instead hardcoded a per-package branch internally, these two
    // would keep passing with today's call sites but silently diverge the
    // moment either call site's onEmpty argument changed -- these prove
    // the flag itself is load-bearing right now, not incidentally
    // correct.
    [[], 'brace-expansion', true, true],
    [[], 'fast-uri', false, false],
  ])('allPatched(%j, %s, onEmpty=%s) === %s', (versions, packageName, onEmpty, expected) => {
    expect(allPatched(versions, packageName, { onEmpty })).toBe(expected);
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
