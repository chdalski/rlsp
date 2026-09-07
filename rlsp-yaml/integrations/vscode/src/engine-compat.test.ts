import { readFileSync } from 'fs';
import * as path from 'path';
import { describe, expect, it } from 'vitest';

// Regression guard for `engines.vscode` and `devDependencies['@types/vscode']`
// in package.json staying in lockstep. `@vscode/vsce` (out/package.js,
// `validateVSCodeTypesCompatibility` in validation.js) refuses to package
// the extension when @types/vscode's major.minor exceeds engines.vscode's
// -- but only when @types/vscode is present in devDependencies at all;
// drop that dependency and vsce's own check silently stops running. This
// guard is independent of vsce's presence and enforces exact major.minor
// *equality*, which is stricter than vsce's "not ahead" tolerance: this
// project's invariant is lockstep, and mirroring vsce's tolerance would
// let @types/vscode drift *below* engines.vscode without failing, which
// is exactly the kind of silent divergence this guard exists to catch.
// There is no production engine-compat.ts -- same as overrides.test.ts,
// which has no overrides.ts -- this test file *is* the guard.
const packageJsonPath = path.join(__dirname, '..', 'package.json');
const lockfilePath = path.join(__dirname, '..', 'pnpm-lock.yaml');

// JSON.parse returns `any`. Casting that away with `as` would silently
// paper over a missing or malformed field instead of failing loudly with
// a message that names the field -- so every read below goes through a
// runtime shape check instead, the same throw-on-unexpected-shape idiom
// as parseVersion further down.
function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function readStringField(record: Record<string, unknown>, field: string, label: string): string {
  const value = record[field];
  if (typeof value !== 'string') {
    throw new Error(`package.json "${label}" is missing or not a string`);
  }
  return value;
}

function readEnginesVscode(manifest: unknown): string {
  if (!isRecord(manifest)) {
    throw new Error('package.json root is not an object');
  }
  const engines = manifest['engines'];
  if (!isRecord(engines)) {
    throw new Error('package.json is missing "engines.vscode"');
  }
  return readStringField(engines, 'vscode', 'engines.vscode');
}

function readTypesVscode(manifest: unknown): string {
  if (!isRecord(manifest)) {
    throw new Error('package.json root is not an object');
  }
  const devDependencies = manifest['devDependencies'];
  if (!isRecord(devDependencies)) {
    throw new Error(`package.json is missing "devDependencies['@types/vscode']"`);
  }
  return readStringField(devDependencies, '@types/vscode', `devDependencies['@types/vscode']`);
}

// Strips a leading `^` or `~` and requires exactly major.minor.patch.
// Deliberately does not support `x` wildcards (e.g. `^1.x.0`) -- this
// project has never used wildcard ranges for these fields, and tolerating
// a form it does not use is speculative generality.
function parseVersion(version: string): [number, number] {
  const match = /^[\^~]?(\d+)\.(\d+)\.(\d+)$/.exec(version);
  if (!match) {
    throw new Error(`unexpected version format: ${version}`);
  }
  const [, major, minor] = match;
  return [Number(major), Number(minor)];
}

// Exact major.minor equality. vsce's own check (see the top-of-file
// comment) only rejects @types/vscode when it is *ahead* of
// engines.vscode and compares major.minor only, ignoring patch --
// mirrored here for the "ignoring patch" part, but not for the
// direction: this guard fails on drift in *either* direction.
function majorMinorEqual(a: string, b: string): boolean {
  const [aMajor, aMinor] = parseVersion(a);
  const [bMajor, bMinor] = parseVersion(b);
  return aMajor === bMajor && aMinor === bMinor;
}

// Windows checkouts of this repository read text files with CRLF line
// endings unless normalized at checkout (see ../.gitattributes, which
// pins pnpm-lock.yaml to LF as the checkout-time fix). Duplicated from
// overrides.test.ts rather than imported -- no precedent for importing
// helpers across test files here, and it would couple this guard to
// Task 1's file for a 3-line utility. Three similar lines beat that
// coupling.
function normalizeLineEndings(text: string): string {
  return text.replace(/\r\n/g, '\n');
}

// @types/vscode is a root devDependency, so it resolves to exactly one
// version in the lockfile -- unlike a helper built to collect every
// resolved version of a package that can appear at multiple graph
// positions, this returns a single string and throws if the header is
// missing or unexpectedly appears more than once, rather than silently
// picking one.
function resolvedTypesVscodeVersion(lockfileText: string): string {
  const headerPattern = /^ {2}'@types\/vscode@(\d+\.\d+\.\d+)':$/gm;
  const matches = [...normalizeLineEndings(lockfileText).matchAll(headerPattern)];
  if (matches.length === 0) {
    throw new Error("pnpm-lock.yaml has no '@types/vscode' package header");
  }
  if (matches.length > 1) {
    throw new Error("pnpm-lock.yaml has more than one '@types/vscode' package header");
  }
  const [match] = matches;
  const version = match?.[1];
  if (version === undefined) {
    throw new Error('unexpected header match with no captured version');
  }
  return version;
}

describe('engines.vscode / @types/vscode lockstep guard', () => {
  const packageJson: unknown = JSON.parse(readFileSync(packageJsonPath, 'utf8'));
  const lockfile = readFileSync(lockfilePath, 'utf8');

  it('engines.vscode is present and non-empty', () => {
    expect(readEnginesVscode(packageJson).length).toBeGreaterThan(0);
  });

  it("devDependencies['@types/vscode'] is present and non-empty", () => {
    expect(readTypesVscode(packageJson).length).toBeGreaterThan(0);
  });

  it('the two real values agree on major.minor', () => {
    // Compare the two read values to each other -- not against a
    // hardcoded "1.125" literal -- so this test survives a future
    // deliberate bump of both fields together.
    const enginesVscode = readEnginesVscode(packageJson);
    const typesVscode = readTypesVscode(packageJson);
    expect(majorMinorEqual(typesVscode, enginesVscode)).toBe(true);
  });

  it('the @types/vscode specifier is an exact pin, not a range', () => {
    // @types/vscode publishes a version per VS Code minor and never
    // bumps its major past 1, so a caret (or tilde) range here is
    // unbounded: every future release satisfies it forever. That is the
    // exact mechanism that let this drift silently more than once
    // (^1.125.0 -> ^1.134.0 -> ^1.136.0, each a semver-valid caret
    // bump). An exact pin is the only specifier form for this package
    // where "declared floor" and "what can install" are the same value.
    const typesVscode = readTypesVscode(packageJson);
    expect(typesVscode).toMatch(/^\d+\.\d+\.\d+$/);
  });

  it('pnpm-lock.yaml resolves @types/vscode to the exact version pinned in package.json', () => {
    // Kept as its own assertion, separate from "agree on major.minor"
    // above, so a failure tells you which file is at fault: this one
    // fails when the lockfile is stale relative to an already-correct
    // package.json specifier; the major.minor test fails when the
    // specifier itself is wrong relative to engines.vscode. CI runs
    // plain `pnpm install` (no --frozen-lockfile), so resolution
    // self-heals inside CI and a stale committed lockfile would
    // otherwise survive indefinitely -- this is the only check in the
    // pipeline that would catch it.
    const typesVscode = readTypesVscode(packageJson);
    expect(resolvedTypesVscodeVersion(lockfile)).toBe(typesVscode);
  });
});

// Direct unit coverage for majorMinorEqual/parseVersion. The
// manifest-driven tests above only ever exercise these against whatever
// package.json currently declares -- once the two fields are fixed, the
// "agree on major.minor" test above can never exercise the false branch
// again. These cases are the standing proof the comparison itself can
// fail, independent of what package.json currently declares. Same
// relationship as isAtLeast's literal suite to the lockfile-driven
// fast-uri test in overrides.test.ts.
describe('majorMinorEqual', () => {
  it.each([
    // Patch differences are not load-bearing.
    ['^1.125.0', '^1.125.7', true],
    ['1.125.0', '1.125.0', true],
    // Today's exact broken state, preserved as a standing literal.
    ['^1.136.0', '^1.125.0', false],
    ['^2.0.0', '^1.125.0', false],
    // Types *below* engine: vsce's own check would accept this (it only
    // rejects types being ahead); this guard must not.
    ['^1.120.0', '^1.125.0', false],
  ])('majorMinorEqual(%s, %s) === %s', (a, b, expected) => {
    expect(majorMinorEqual(a, b)).toBe(expected);
  });
});

// Direct unit coverage for resolvedTypesVscodeVersion, against literal
// lockfile snippets rather than the real pnpm-lock.yaml -- once the
// lockfile is regenerated to match the pinned specifier, the real-file
// test above can never exercise the "header absent" branch again. These
// two stay provably able to fail independent of what pnpm-lock.yaml
// currently resolves.
describe('resolvedTypesVscodeVersion', () => {
  it('reads the version out of a package header line', () => {
    const snippet = [
      "  '@types/sarif@2.1.7':",
      '    resolution: {integrity: sha512-fake==}',
      '',
      "  '@types/vscode@1.125.0':",
      '    resolution: {integrity: sha512-fake==}',
      '',
    ].join('\n');
    expect(resolvedTypesVscodeVersion(snippet)).toBe('1.125.0');
  });

  it('throws when the header is absent', () => {
    const snippet = [
      "  '@types/sarif@2.1.7':",
      '    resolution: {integrity: sha512-fake==}',
      '',
    ].join('\n');
    expect(() => resolvedTypesVscodeVersion(snippet)).toThrow("no '@types/vscode' package header");
  });
});

describe('parseVersion', () => {
  it('parses a caret-prefixed version', () => {
    expect(parseVersion('^1.125.0')).toEqual([1, 125]);
  });

  it('parses a tilde-prefixed version', () => {
    expect(parseVersion('~1.125.3')).toEqual([1, 125]);
  });

  it('parses an unprefixed version without rejecting it', () => {
    expect(parseVersion('1.125.0')).toEqual([1, 125]);
  });

  it('throws on an x wildcard component', () => {
    expect(() => parseVersion('^1.x.0')).toThrow('unexpected version format');
  });

  it('throws on a non-version string', () => {
    expect(() => parseVersion('not-a-version')).toThrow('unexpected version format');
  });
});

describe('manifest shape guards', () => {
  it('throws when engines.vscode is missing, naming the field', () => {
    expect(() => readEnginesVscode({})).toThrow('engines.vscode');
  });

  it('throws when engines.vscode is not a string', () => {
    expect(() => readEnginesVscode({ engines: { vscode: 1 } })).toThrow('engines.vscode');
  });

  it('throws when @types/vscode is absent from devDependencies, naming the field', () => {
    expect(() => readTypesVscode({ devDependencies: {} })).toThrow('@types/vscode');
  });

  it('throws when @types/vscode is not a string', () => {
    expect(() => readTypesVscode({ devDependencies: { '@types/vscode': 1 } })).toThrow(
      '@types/vscode',
    );
  });
});
