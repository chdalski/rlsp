import { readFileSync } from 'fs';
import * as path from 'path';
import { describe, expect, it } from 'vitest';

// Regression guard for the pnpm.overrides entries that patch two npm
// security advisories (brace-expansion GHSA-3jxr-9vmj-r5cp, fast-uri
// GHSA-v2hh-gcrm-f6hx / GHSA-4c8g-83qw-93j6). brace-expansion reaches the
// extension's runtime dependency path via vscode-languageclient ->
// minimatch@5.1.9, so this asserts the lockfile actually resolves a
// non-vulnerable version -- not just that an override string is present.
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
const lockfile = readFileSync(lockfilePath, 'utf8');

// The lockfile lists each package twice: once under `packages:` (resolution
// metadata only) and once under `snapshots:` (resolved `dependencies:`).
// Use the last occurrence to read the resolved dependency version.
function resolvedDependencyVersion(
  blockHeader: string,
  dependencyName: string,
): string | undefined {
  const blockStart = lockfile.lastIndexOf(`\n  ${blockHeader}\n`);
  if (blockStart === -1) return undefined;
  const blockEnd = lockfile.indexOf('\n\n', blockStart);
  const block = lockfile.slice(blockStart, blockEnd === -1 ? undefined : blockEnd);
  const match = new RegExp(`${dependencyName}: (\\S+)`).exec(block);
  return match?.[1];
}

// Every `<packageName>@X.Y.Z:` header in the lockfile, deduplicated. Used to
// check a package across every resolved version simultaneously present in
// the graph, not just the one an override would have forced.
function allResolvedVersions(packageName: string): string[] {
  const headerPattern = new RegExp(`\\n  ${packageName}@(\\d+\\.\\d+\\.\\d+):`, 'g');
  const versions = new Set<string>();
  for (const match of lockfile.matchAll(headerPattern)) {
    versions.add(match[1]);
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
  const a = parseVersion(actual);
  const f = parseVersion(floor);
  for (let i = 0; i < 3; i++) {
    if (a[i] !== f[i]) return a[i] > f[i];
  }
  return true;
}

describe('pnpm.overrides regression guard (brace-expansion / fast-uri)', () => {
  it('overrides block declares the retained pins', () => {
    const overridesStart = lockfile.indexOf('overrides:');
    const overridesEnd = lockfile.indexOf('\n\n', overridesStart);
    const overridesBlock = lockfile.slice(overridesStart, overridesEnd);
    expect(overridesBlock).toContain('brace-expansion@5: ^5.0.9');
    expect(overridesBlock).toContain('serialize-javascript: ^7.0.5');
  });

  it('overrides block no longer declares the removed brace-expansion@2 / fast-uri pins', () => {
    const overridesStart = lockfile.indexOf('overrides:');
    const overridesEnd = lockfile.indexOf('\n\n', overridesStart);
    const overridesBlock = lockfile.slice(overridesStart, overridesEnd);
    expect(overridesBlock).not.toContain('brace-expansion@2:');
    expect(overridesBlock).not.toContain('fast-uri:');
  });

  it('brace-expansion on minimatch@5.1.9 resolves to a non-vulnerable version', () => {
    const resolved = resolvedDependencyVersion('minimatch@5.1.9:', 'brace-expansion');
    expect(resolved).toBeDefined();
    expect(isAtLeast(resolved ?? '', '2.1.4')).toBe(true);
  });

  it('brace-expansion on minimatch@9.0.9 resolves to a non-vulnerable version', () => {
    const resolved = resolvedDependencyVersion('minimatch@9.0.9:', 'brace-expansion');
    expect(resolved).toBeDefined();
    expect(isAtLeast(resolved ?? '', '2.1.4')).toBe(true);
  });

  it('brace-expansion on minimatch@10.2.5 remains pinned to the overridden major', () => {
    expect(resolvedDependencyVersion('minimatch@10.2.5:', 'brace-expansion')).toBe('5.0.9');
  });

  it('fast-uri resolves to a non-vulnerable version everywhere in the lockfile, with the override removed', () => {
    const versions = allResolvedVersions('fast-uri');
    expect(versions.length).toBeGreaterThan(0);
    for (const version of versions) {
      expect(isAtLeast(version, '3.1.5')).toBe(true);
    }
  });
});
