import { readFileSync } from 'fs';
import * as path from 'path';
import { describe, expect, it } from 'vitest';

// Regression guard for the pnpm.overrides entries that patch two npm
// security advisories (brace-expansion GHSA-3jxr-9vmj-r5cp, fast-uri
// GHSA-v2hh-gcrm-f6hx / GHSA-4c8g-83qw-93j6). brace-expansion reaches the
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
      expect(isAtLeast(version, '3.1.5')).toBe(true);
    }
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
