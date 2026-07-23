import { readFileSync } from 'fs';
import * as path from 'path';
import { describe, expect, it } from 'vitest';

// Regression guard for the pnpm.overrides entries that patch two npm
// security advisories (brace-expansion GHSA-3jxr-9vmj-r5cp, fast-uri
// GHSA-v2hh-gcrm-f6hx / GHSA-4c8g-83qw-93j6). brace-expansion reaches the
// extension's runtime dependency path via vscode-languageclient ->
// minimatch@5.1.9, so this asserts the lockfile actually resolves the
// override -- not just that the override string is present.
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

describe('pnpm.overrides regression guard (brace-expansion / fast-uri)', () => {
  it('overrides block declares brace-expansion@2 and the updated fast-uri range', () => {
    const overridesStart = lockfile.indexOf('overrides:');
    const overridesEnd = lockfile.indexOf('\n\n', overridesStart);
    const overridesBlock = lockfile.slice(overridesStart, overridesEnd);
    expect(overridesBlock).toContain('brace-expansion@2: ^2.1.2');
    expect(overridesBlock).toContain('brace-expansion@5: ^5.0.6');
    expect(overridesBlock).toContain('fast-uri: ^3.1.4');
  });

  it('brace-expansion resolves to the patched version on the minimatch@5.1.9 runtime path', () => {
    expect(resolvedDependencyVersion('minimatch@5.1.9:', 'brace-expansion')).toBe('2.1.2');
  });

  it('brace-expansion resolves to the patched version on the minimatch@9.0.9 dev path', () => {
    expect(resolvedDependencyVersion('minimatch@9.0.9:', 'brace-expansion')).toBe('2.1.2');
  });

  it('brace-expansion on the minimatch@10.2.5 path is unaffected by the @2 override', () => {
    expect(resolvedDependencyVersion('minimatch@10.2.5:', 'brace-expansion')).toBe('5.0.7');
  });

  it('fast-uri resolves to the patched version with no vulnerable version remaining', () => {
    expect(lockfile).toContain('fast-uri@3.1.4:');
    expect(lockfile).not.toContain('fast-uri@3.1.2');
  });
});
