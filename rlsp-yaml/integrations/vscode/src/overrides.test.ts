import { readFileSync } from 'fs';
import * as path from 'path';
import { describe, expect, it } from 'vitest';

// Regression guard for the pnpm.overrides entries that patch npm
// security advisories (brace-expansion GHSA-3jxr-9vmj-r5cp, fast-uri
// GHSA-5jgf-p345-68v8 / GHSA-fph4-wmhf-6fwf / GHSA-f65p-4m7j-42xc /
// GHSA-jqff-g426-hqxp -- each of these four advisories spans three
// vulnerable ranges on the 2.x, 3.x, and 4.x lines, with patched floors
// 2.4.5, 3.1.6, and 4.1.3 respectively). fast-uri carries no override --
// its version drifts with ordinary transitive dependency updates -- so
// isPatchedFastUri() below encodes both verified patched floors (3.1.6
// and 4.1.3) directly, and fails closed for every other major line,
// including 2.x, even though 2.4.5 is also patched: a transitive
// downgrade across a major line is unexpected enough that it should stop
// and be looked at rather than pass on an assumption. brace-expansion reaches the
// extension's runtime dependency path transitively through
// vscode-languageclient's dependency graph (currently via minimatch, though
// the exact minimatch version is not load-bearing for this guard -- see the
// brace-expansion assertions below, which key off brace-expansion's own
// resolved versions rather than any specific minimatch version), so this
// asserts the lockfile actually resolves a non-vulnerable version -- not
// just that an override string is present.
//
// brace-expansion@2 and fast-uri no longer have overrides: the
// 2026-08-07 overrides-consolidation audit proved their dependency chains
// resolve non-vulnerable versions on their own. Their guard tests below
// assert the resolved version directly (a semver floor, not the old
// override-was-applied exact match) so the guard still fails if a future
// dependency bump regresses into the advisory range. brace-expansion@5
// remains overridden (deferred DoS fix, GHSA-mh99-v99m-4gvg /
// GHSA-rgw5-rvv9-x895) and keeps its exact-match assertion, since an
// override change is a visible, intentional edit to the overrides block
// rather than silent transitive drift.
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

describe('pnpm.overrides regression guard (brace-expansion / fast-uri)', () => {
  it('overrides block declares the retained pins', () => {
    const overridesBlock = overridesBlockOf(lockfile);
    expect(overridesBlock).toContain('brace-expansion@5: ^5.0.9');
    expect(overridesBlock).toContain('serialize-javascript: ^7.0.5');
  });

  it('overrides block no longer declares the removed brace-expansion@2 / fast-uri pins', () => {
    const overridesBlock = overridesBlockOf(lockfile);
    expect(overridesBlock).not.toContain('brace-expansion@2:');
    expect(overridesBlock).not.toContain('fast-uri:');
  });

  it('brace-expansion resolves to a non-vulnerable version on every non-overridden branch of the graph', () => {
    const nonOverriddenVersions = allResolvedVersions(lockfile, 'brace-expansion').filter(
      (version) => parseVersion(version)[0] !== 5,
    );
    expect(nonOverriddenVersions.length).toBeGreaterThan(0);
    for (const version of nonOverriddenVersions) {
      expect(isAtLeast(version, '2.1.4')).toBe(true);
    }
  });

  it("brace-expansion's overridden major-5 branch stays pinned to the audited patched version", () => {
    const overriddenVersions = allResolvedVersions(lockfile, 'brace-expansion').filter(
      (version) => parseVersion(version)[0] === 5,
    );
    expect(overriddenVersions).toEqual(['5.0.9']);
  });

  it('fast-uri resolves to a non-vulnerable version everywhere in the lockfile, with the override removed', () => {
    const versions = allResolvedVersions(lockfile, 'fast-uri');
    expect(versions.length).toBeGreaterThan(0);
    for (const version of versions) {
      expect(isPatchedFastUri(version)).toBe(true);
    }
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
