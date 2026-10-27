import { readFileSync } from 'fs';
import * as path from 'path';
import { describe, expect, it } from 'vitest';

// Regression guard for the pnpm.overrides entries that patch npm security
// advisories. Verified directly against the GitHub Advisory API:
// - brace-expansion is covered by GHSA-6j4f-fj2g-mc7p (HIGH,
//   CVE-2026-102276), GHSA-qhr7-859c-m2p7 (HIGH, CVE-2026-102278) and
//   GHSA-q2hr-2g5m-vwhr (MEDIUM, CVE-2026-102277). The binding floor per
//   major line is the highest of the three, the one set by
//   GHSA-q2hr-2g5m-vwhr: `>= 2.0.0, < 2.1.7` (patched 2.1.7) and
//   `>= 4.0.0, < 5.0.12` (patched 5.0.12). The same advisories also
//   publish `< 1.1.21` (patched 1.1.21) and `>= 3.0.0, < 3.0.9` (patched
//   3.0.9). Major 4 has no patched release at all -- every 4.x version is
//   vulnerable and the fix is moving to major 5, so a missing major-4
//   floor below means migrating off major 4 entirely, not that no floor
//   has been found yet.
// - serialize-javascript is covered by GHSA-m5mm-p8h7-m29v (LOW,
//   CVE-2026-115897): `< 7.1.3`, patched 7.1.3 -- the binding advisory for
//   the floor. It also covers the range of the earlier GHSA-gfhx-hw2g-v5hg
//   (LOW, CVE-2026-97711): `>= 7.1.1, < 7.1.2`, patched 7.1.2. The override
//   also forces @vscode/test-cli's mocha off its declared ^6.0.2 onto
//   major 7, because the 6.x line carries older advisories.
// - source-map-js is covered by GHSA-68fv-2mgg-jv7q (HIGH,
//   CVE-2026-93749): `>= 1.0.0, < 1.2.2`, patched 1.2.2.
// - fast-uri is covered by GHSA-5jgf-p345-68v8 / GHSA-fph4-wmhf-6fwf /
//   GHSA-f65p-4m7j-42xc / GHSA-jqff-g426-hqxp, with verified patched
//   floors of 3.1.6 and 4.1.3.
// All of the above is reference data only, not executable acceptance --
// see the patchedFloors table and isPatchedVersion below for which of
// these floors this guard actually checks against.
//
// Before retiring any override: re-derive the current direct
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
// package, not every major the advisories have patched -- for example
// brace-expansion majors 1 and 3, and fast-uri major 2, each covered by
// their own advisory floor but with no entry in that table. A major
// absent from a package's row is unvetted, not safe, and the lookup
// fails closed on it rather than falling back to another major or
// another package; see patchedFloors/isPatchedVersion below for the
// exact mechanics. The resolved-major-lines canary test below
// independently fails the moment an unvetted brace-expansion major
// appears in the lockfile at all; fast-uri, serialize-javascript and
// source-map-js have no equivalent canary, so the fail-closed lookup is
// their only backstop.

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

describe('normalizeLineEndings', () => {
  it('converts CRLF to LF without corrupting content', () => {
    const input = 'a:\r\n  b: 1\r\n\r\nc:\r\n  d: 2\r\n';
    const expected = 'a:\n  b: 1\n\nc:\n  d: 2\n';
    expect(normalizeLineEndings(input)).toBe(expected);
  });

  it('is a no-op on already-LF content', () => {
    const input =
      'packages:\n\n  minimatch@5.1.9:\n    dependencies:\n      brace-expansion: 2.1.7\n';
    expect(normalizeLineEndings(input)).toBe(input);
  });
});

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

// Direct unit coverage for isAtLeast/parseVersion. The lockfile-driven
// tests below only ever exercise these against whatever fast-uri/
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
    // The brace-expansion@5 patched floor (GHSA-q2hr-2g5m-vwhr): the
    // override's caret range (^5.0.12) permits ordinary patch drift above
    // this floor, but allPatched below must still catch drift
    // below it.
    ['5.0.11', '5.0.12', false],
    ['5.0.12', '5.0.12', true],
    // Numeric comparison again: '5.0.9' > '5.0.12' as strings, but 9 < 12
    // as numbers.
    ['5.0.9', '5.0.12', false],
    ['7.1.10', '7.1.3', true],
  ])('isAtLeast(%s, %s) === %s', (actual, floor, expected) => {
    expect(isAtLeast(actual, floor)).toBe(expected);
  });
});

describe('parseVersion', () => {
  it('throws on a non-release version string', () => {
    expect(() => parseVersion('3.1.6-beta.1')).toThrow('unexpected version format');
  });
});

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
// the packages this guard vets, so a typo in the package name is a
// compile error rather than a silent miss.
type GuardedPackage = 'brace-expansion' | 'fast-uri' | 'serialize-javascript' | 'source-map-js';

// The single declared table of patched floors. brace-expansion majors 2
// and 5 are the two lines reachable in this graph; their floors are set by
// GHSA-q2hr-2g5m-vwhr, the newest of GHSA-6j4f-fj2g-mc7p /
// GHSA-qhr7-859c-m2p7 / GHSA-q2hr-2g5m-vwhr (see header comment for the
// advisories' full floor list, most of which is deliberately not wired in
// here). fast-uri majors 3 and 4 are its two verified patched floors
// (GHSA-5jgf-p345-68v8 / GHSA-fph4-wmhf-6fwf / GHSA-f65p-4m7j-42xc /
// GHSA-jqff-g426-hqxp). serialize-javascript major 7 is the only line the
// override permits; its floor is set by GHSA-m5mm-p8h7-m29v, which supersedes
// the earlier GHSA-gfhx-hw2g-v5hg floor of 7.1.2. source-map-js
// major 1 is its only published line; its floor is set by
// GHSA-68fv-2mgg-jv7q. Any major line absent from a package's row --
// including majors the advisories cover with floors of their own, like
// brace-expansion 1 and 3, or fast-uri 2 -- has no entry, and
// isPatchedVersion below fails closed on a missing entry rather than
// falling back to any other row or major.
const patchedFloors: Record<GuardedPackage, Record<number, string>> = {
  'brace-expansion': { 2: '2.1.7', 5: '5.0.12' },
  'fast-uri': { 3: '3.1.6', 4: '4.1.3' },
  'serialize-javascript': { 7: '7.1.3' },
  'source-map-js': { 1: '1.2.2' },
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

// Boundary coverage for isPatchedVersion, independent of what the lockfile
// currently resolves. Union of every package's patched floors and
// false-positive traps, the fail-closed default for majors this guard has
// not vetted for each package, and the cross-contamination traps: each
// package's majors are disjoint from the others', so every row that applies
// one package's real floor to another package must be false -- a table
// keyed by major only (dropping packageName) would wrongly flip them to
// true. There is no runtime "unknown package name" case here:
// GuardedPackage's literal union type makes that a compile error, not a
// reachable branch.
describe('isPatchedVersion', () => {
  it.each<[GuardedPackage, string, boolean]>([
    // fast-uri covers both patched floors and the two 4.x false-positive
    // traps (4.0.0 is major-4 but pre-floor; 4.1.2 is one patch below the
    // floor) that isAtLeast alone would miss.
    ['fast-uri', '3.1.5', false],
    ['fast-uri', '3.1.6', true],
    ['fast-uri', '4.0.0', false],
    ['fast-uri', '4.1.2', false],
    ['fast-uri', '4.1.3', true],
    ['fast-uri', '4.2.0', true],
    ['fast-uri', '2.9.9', false],
    ['fast-uri', '5.0.0', false],
    // Trap: major 1's advisory-patched floor (GHSA-q2hr-2g5m-vwhr), but
    // major 1 has no dependent in this graph, so it must stay unvetted --
    // same reasoning as fast-uri's major-2 case above.
    ['brace-expansion', '1.1.21', false],
    // One patch below each brace-expansion floor, then the floor itself.
    ['brace-expansion', '2.1.6', false],
    ['brace-expansion', '2.1.7', true],
    ['brace-expansion', '2.5.0', true],
    // Trap: major 3's advisory-patched floor (GHSA-q2hr-2g5m-vwhr) -- must
    // still reject, for the same reason as 1.1.21 above.
    ['brace-expansion', '3.0.9', false],
    // Major 4's lowest version -- no patched release exists on this line.
    ['brace-expansion', '4.0.0', false],
    // High into major 4, still vulnerable -- proves this isn't
    // accidentally implemented as a floor check (e.g. isAtLeast(v,
    // '4.0.0'), which would wrongly return true here).
    ['brace-expansion', '4.9.9', false],
    ['brace-expansion', '5.0.11', false],
    ['brace-expansion', '5.0.12', true],
    ['brace-expansion', '5.1.0', true],
    // Fail-closed default: major 0 also falls inside the advisories' real
    // `< 1.1.21` band, so this is doubly correct, not just unvetted.
    ['brace-expansion', '0.9.9', false],
    // Fail-closed default for a future major the advisories don't cover
    // at all.
    ['brace-expansion', '6.0.0', false],
    // serialize-javascript: one patch below the floor (7.1.2, the previous
    // floor, vulnerable per GHSA-m5mm-p8h7-m29v), the version
    // GHSA-gfhx-hw2g-v5hg names, a lower minor, the floor,
    // numeric-not-lexicographic, and majors with no floor (6.9.9 is the line
    // mocha declares; 8.0.0 is a future major).
    ['serialize-javascript', '7.1.2', false],
    ['serialize-javascript', '7.1.1', false],
    ['serialize-javascript', '7.0.9', false],
    ['serialize-javascript', '7.1.3', true],
    ['serialize-javascript', '7.1.10', true],
    ['serialize-javascript', '7.2.0', true],
    ['serialize-javascript', '6.9.9', false],
    ['serialize-javascript', '8.0.0', false],
    // source-map-js: same shapes against GHSA-68fv-2mgg-jv7q's floor.
    ['source-map-js', '1.2.1', false],
    ['source-map-js', '1.1.9', false],
    ['source-map-js', '1.2.2', true],
    ['source-map-js', '1.2.10', true],
    ['source-map-js', '1.3.0', true],
    ['source-map-js', '0.9.9', false],
    ['source-map-js', '2.0.0', false],
    // Cross-contamination traps: every other package's real patched floors,
    // applied to each package. fast-uri's major-3/4 floors against
    // brace-expansion, which has no major-3 floor at all and a different
    // (fully-vulnerable) major-4 status.
    ['brace-expansion', '3.1.6', false],
    ['brace-expansion', '4.1.3', false],
    ['brace-expansion', '1.2.2', false],
    ['brace-expansion', '7.1.3', false],
    ['fast-uri', '2.1.7', false],
    ['fast-uri', '5.0.12', false],
    ['fast-uri', '1.2.2', false],
    ['fast-uri', '7.1.3', false],
    ['serialize-javascript', '1.2.2', false],
    ['serialize-javascript', '2.1.7', false],
    ['serialize-javascript', '3.1.6', false],
    ['serialize-javascript', '4.1.3', false],
    ['serialize-javascript', '5.0.12', false],
    ['source-map-js', '2.1.7', false],
    ['source-map-js', '3.1.6', false],
    ['source-map-js', '4.1.3', false],
    ['source-map-js', '5.0.12', false],
    ['source-map-js', '7.1.3', false],
  ])('isPatchedVersion(%s, %s) === %s', (packageName, version, expected) => {
    expect(isPatchedVersion(packageName, version)).toBe(expected);
  });
});

// Single aggregation decision point: every resolved version of the named
// package must be patched, and the empty-set outcome is supplied by the
// caller via the named onEmpty field rather than hardcoded per package --
// a named field keeps the polarity legible at each call site, where a bare
// positional boolean would not be. The brace-expansion,
// serialize-javascript and source-map-js call sites pass onEmpty: false,
// because their overridden packages are expected always to resolve -- an
// empty set means an override silently went dead, which must fail loudly
// rather than pass vacuously on `[].every(...)`. The
// fast-uri call site passes onEmpty: true, because fast-uri carries no
// override and its absence from the current graph is tolerated. See
// header comment for the full reasoning behind each policy.
function allPatched(
  versions: string[],
  packageName: GuardedPackage,
  { onEmpty }: { onEmpty: boolean },
): boolean {
  if (versions.length === 0) return onEmpty;
  return versions.every((version) => isPatchedVersion(packageName, version));
}

// Boundary coverage for allPatched's aggregation semantics, independent of
// what the lockfile currently resolves. isPatchedVersion's own floor/major
// comparisons are already covered above; these cases are scoped to the
// aggregation behavior only (empty-array, mixed-major array, and the
// onEmpty flag itself). Covers every package's aggregation cases, plus two
// traps at the end that pass the *other* policy alongside an empty array --
// proving onEmpty itself controls the outcome rather than a hardcoded
// per-package branch that happens to match today's call sites.
describe('allPatched', () => {
  it.each<[string[], GuardedPackage, boolean, boolean]>([
    // brace-expansion (onEmpty: false throughout).
    [['2.1.7'], 'brace-expansion', false, true],
    [['2.1.8'], 'brace-expansion', false, true],
    // One patch below the floor: the top of minimatch@9.0.9's admitted
    // range (^2.0.2) reaches into this vulnerable band.
    [['2.1.6'], 'brace-expansion', false, false],
    // Inclusive floor, single element -- the shape of today's real
    // lockfile major-5 branch.
    [['5.0.12'], 'brace-expansion', false, true],
    [['5.0.11'], 'brace-expansion', false, false],
    // Mixed majors, both patched -- proves the aggregator honors each
    // element's own major-dispatched floor rather than one global floor,
    // which could not express two simultaneously-valid floors at once.
    [['2.1.7', '5.0.12'], 'brace-expansion', false, true],
    // Mixed majors, one bad -- proves every element is checked, not just
    // the first or the last.
    [['2.1.7', '5.0.11'], 'brace-expansion', false, false],
    [['2.1.6', '5.0.12'], 'brace-expansion', false, false],
    // A major-4 version on its own -- proves the aggregator doesn't
    // bypass isPatchedVersion's always-false brace-expansion major-4
    // handling.
    [['4.0.0'], 'brace-expansion', false, false],
    // Empty must fail: every overridden package is expected to resolve
    // (see header comment on overrides.test.ts), so a vanished branch is
    // itself a regression, not a pass.
    [[], 'brace-expansion', false, false],
    // serialize-javascript (onEmpty: false throughout).
    [['7.1.3'], 'serialize-javascript', false, true],
    // The previous floor, which the lockfile would keep if the override
    // stayed at ^7.1.2.
    [['7.1.2'], 'serialize-javascript', false, false],
    [['7.1.1'], 'serialize-javascript', false, false],
    // The major mocha declares, which resolves if the override dies.
    [['6.0.2'], 'serialize-javascript', false, false],
    // A patched sibling does not excuse an unchecked major.
    [['7.1.3', '6.0.2'], 'serialize-javascript', false, false],
    [[], 'serialize-javascript', false, false],
    // source-map-js (onEmpty: false throughout).
    [['1.2.2'], 'source-map-js', false, true],
    [['1.2.1'], 'source-map-js', false, false],
    [['2.0.0'], 'source-map-js', false, false],
    [['1.2.2', '2.0.0'], 'source-map-js', false, false],
    [[], 'source-map-js', false, false],
    // fast-uri (onEmpty: true throughout).
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
    // onEmpty-flag traps: an empty array with the *other* package's
    // historical policy. If allPatched ignored onEmpty
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

const lockfilePath = path.join(__dirname, '..', 'pnpm-lock.yaml');

const lockfile = normalizeLineEndings(readFileSync(lockfilePath, 'utf8'));

// Slice the `overrides:` block out of a lockfile string.
function overridesBlockOf(text: string): string {
  const overridesStart = text.indexOf('overrides:');
  const overridesEnd = text.indexOf('\n\n', overridesStart);
  return text.slice(overridesStart, overridesEnd === -1 ? undefined : overridesEnd);
}

// The `overrides:` block as a key -> range map, so the pins can be asserted
// by exact equality: a substring match would also accept `^7.1.30` for
// `^7.1.3`, or a longer key that merely ends in the expected one.
function overridesOf(text: string): Record<string, string> {
  const entries: Record<string, string> = {};
  for (const line of overridesBlockOf(text).split('\n').slice(1)) {
    const match = /^ {2}(\S+): (\S+)$/.exec(line);
    if (!match?.[1] || !match[2]) {
      throw new Error(`unexpected line in overrides block: ${line}`);
    }
    entries[match[1]] = match[2];
  }
  return entries;
}

// Every `<packageName>@<version>:` header in the lockfile, deduplicated.
// Used to check a package across every resolved version simultaneously
// present in the graph, not just the one an override would have forced.
// The version token is captured as-is rather than constrained to
// `X.Y.Z`: a pre-release tag or a URL-sourced spec must reach
// parseVersion and fail loudly, because silently dropping it here would
// shrink the set this security guard checks. Peer-suffixed snapshot
// headers (`name@1.2.3(peer@4):`) capture the same version as the plain
// `packages:` key and are deduplicated. Scanning starts at `packages:` so
// the `overrides:` block's own `name@major: range` keys are not mistaken for
// resolved versions.
function allResolvedVersions(lockfileText: string, packageName: string): string[] {
  const packagesStart = lockfileText.search(/^packages:$/m);
  if (packagesStart === -1) {
    throw new Error('lockfile has no packages: section');
  }
  const text = lockfileText.slice(packagesStart);
  const escapedName = packageName.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const headerPattern = new RegExp(`\\n  ${escapedName}@([^\\s:(]+)(?=[:(])`, 'g');
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

describe('allResolvedVersions', () => {
  const fragment = [
    'packages:',
    '',
    '  serialize-javascript@7.1.3:',
    '    resolution: {}',
    '',
    '  serialize-javascript@7.2.0-beta.0:',
    '    resolution: {}',
    '',
    '  foo@1.0.0(bar@2.0.0):',
    '  foo@1.0.0:',
    '  fooXbar@3.0.0:',
    '  foo.bar@4.0.0:',
  ].join('\n');

  it('captures a pre-release header so parseVersion can reject it', () => {
    const versions = allResolvedVersions(fragment, 'serialize-javascript');
    expect(versions).toEqual(['7.1.3', '7.2.0-beta.0']);
    expect(() => allPatched(versions, 'serialize-javascript', { onEmpty: false })).toThrow(
      'unexpected version format',
    );
  });

  it('deduplicates peer-suffixed headers', () => {
    expect(allResolvedVersions(fragment, 'foo')).toEqual(['1.0.0']);
  });

  it('treats dots in the package name literally', () => {
    expect(allResolvedVersions(fragment, 'foo.bar')).toEqual(['4.0.0']);
  });
});

describe(
  'pnpm.overrides regression guard (brace-expansion / fast-uri / serialize-javascript / ' +
    'source-map-js)',
  () => {
    it('overrides block declares exactly the retained pins', () => {
      expect(overridesOf(lockfile)).toEqual({
        'serialize-javascript': '^7.1.3',
        'source-map-js': '^1.2.2',
        'brace-expansion@2': '^2.1.7',
        'brace-expansion@5': '^5.0.12',
      });
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

    // Not filtered by major: every resolved version must pass, so a stray
    // version on any line the table does not vet fails closed.
    it('serialize-javascript is present and every resolved version is patched', () => {
      const versions = allResolvedVersions(lockfile, 'serialize-javascript');
      expect(allPatched(versions, 'serialize-javascript', { onEmpty: false })).toBe(true);
    });

    it('source-map-js is present and every resolved version is patched', () => {
      const versions = allResolvedVersions(lockfile, 'source-map-js');
      expect(allPatched(versions, 'source-map-js', { onEmpty: false })).toBe(true);
    });

    it('fast-uri is absent from the lockfile, or resolves to a patched version wherever present', () => {
      const versions = allResolvedVersions(lockfile, 'fast-uri');
      expect(allPatched(versions, 'fast-uri', { onEmpty: true })).toBe(true);
    });
  },
);
