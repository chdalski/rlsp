import * as path from 'path';
import { describe, expect, it } from 'vitest';
import { findServerBinary } from './server.js';

const EXT = '/some/ext/dir';

describe('findServerBinary', () => {
  describe('custom server path override', () => {
    it('returns custom path as-is when non-empty and workspace is trusted', () => {
      const result = findServerBinary(EXT, '/usr/local/bin/rlsp-yaml', true, 'linux', 'x64');
      expect(result).toBe(path.resolve('/usr/local/bin/rlsp-yaml'));
    });

    it('falls through to bundled binary when serverPath is whitespace-only', () => {
      const result = findServerBinary(EXT, '   ', true, 'linux', 'x64');
      expect(result).toBe(path.join(EXT, 'server', 'x86_64-unknown-linux-gnu', 'rlsp-yaml'));
    });

    it('falls through to bundled binary when serverPath is empty string', () => {
      const result = findServerBinary(EXT, '', true, 'linux', 'x64');
      expect(result).toBe(path.join(EXT, 'server', 'x86_64-unknown-linux-gnu', 'rlsp-yaml'));
    });

    it('ignores custom path when workspace is not trusted', () => {
      const result = findServerBinary(EXT, '/usr/local/bin/rlsp-yaml', false, 'linux', 'x64');
      expect(result).toBe(path.join(EXT, 'server', 'x86_64-unknown-linux-gnu', 'rlsp-yaml'));
    });
  });

  describe('bundled binary — Linux', () => {
    it('resolves linux x64 to x86_64-unknown-linux-gnu/rlsp-yaml', () => {
      const result = findServerBinary(EXT, '', false, 'linux', 'x64');
      expect(result).toBe(path.join(EXT, 'server', 'x86_64-unknown-linux-gnu', 'rlsp-yaml'));
    });

    it('resolves linux arm64 to aarch64-unknown-linux-gnu/rlsp-yaml', () => {
      const result = findServerBinary(EXT, '', false, 'linux', 'arm64');
      expect(result).toBe(path.join(EXT, 'server', 'aarch64-unknown-linux-gnu', 'rlsp-yaml'));
    });
  });

  describe('bundled binary — macOS', () => {
    it('resolves darwin x64 to x86_64-apple-darwin/rlsp-yaml', () => {
      const result = findServerBinary(EXT, '', false, 'darwin', 'x64');
      expect(result).toBe(path.join(EXT, 'server', 'x86_64-apple-darwin', 'rlsp-yaml'));
    });

    it('resolves darwin arm64 (Apple Silicon) to aarch64-apple-darwin/rlsp-yaml', () => {
      const result = findServerBinary(EXT, '', false, 'darwin', 'arm64');
      expect(result).toBe(path.join(EXT, 'server', 'aarch64-apple-darwin', 'rlsp-yaml'));
    });
  });

  describe('bundled binary — Windows', () => {
    it('resolves win32 x64 to a path ending in .exe', () => {
      const result = findServerBinary(EXT, '', false, 'win32', 'x64');
      expect(result).toMatch(/\.exe$/);
      expect(result).toBe(path.join(EXT, 'server', 'x86_64-pc-windows-msvc', 'rlsp-yaml.exe'));
    });

    it('resolves win32 arm64 to a path ending in .exe', () => {
      const result = findServerBinary(EXT, '', false, 'win32', 'arm64');
      expect(result).toMatch(/\.exe$/);
      expect(result).toBe(path.join(EXT, 'server', 'aarch64-pc-windows-msvc', 'rlsp-yaml.exe'));
    });
  });

  describe('unsupported platforms', () => {
    it('throws a descriptive error for unsupported platform', () => {
      expect(() => findServerBinary(EXT, '', false, 'freebsd', 'x64')).toThrow(
        /unsupported platform "freebsd"/,
      );
    });

    it('throws a descriptive error for unsupported arch on known platform', () => {
      expect(() => findServerBinary(EXT, '', false, 'linux', 'ia32')).toThrow(
        /unsupported architecture "ia32"/,
      );
    });
  });

  describe('path structure', () => {
    it('bundled binary path is rooted under extensionPath', () => {
      const result = findServerBinary(EXT, '', false, 'linux', 'x64');
      expect(result.startsWith(path.join(EXT, ''))).toBe(true);
    });

    it('bundled binary path contains no double separators', () => {
      const extNoTrailing = '/some/ext/dir';
      const result = findServerBinary(extNoTrailing, '', false, 'linux', 'x64');
      expect(result).not.toContain('//');
      if (process.platform === 'win32') {
        // On Windows, only check after drive letter
        expect(result.slice(2)).not.toContain('\\\\');
      }
    });
  });

  describe('security — user-supplied path validation', () => {
    it('rejects UNC paths starting with \\\\', () => {
      expect(() =>
        findServerBinary(EXT, '\\\\attacker\\share\\rlsp-yaml.exe', true, 'win32', 'x64'),
      ).toThrow(/UNC path/);
    });

    it('rejects UNC paths starting with //', () => {
      expect(() =>
        findServerBinary(EXT, '//attacker/share/rlsp-yaml', true, 'linux', 'x64'),
      ).toThrow(/UNC path/);
    });

    it('rejects .cmd extension on Windows', () => {
      expect(() => findServerBinary(EXT, 'C:\\tools\\server.cmd', true, 'win32', 'x64')).toThrow(
        /disallowed extension ".cmd"/,
      );
    });

    it('rejects .bat extension on Windows', () => {
      expect(() => findServerBinary(EXT, 'C:\\tools\\server.bat', true, 'win32', 'x64')).toThrow(
        /disallowed extension ".bat"/,
      );
    });

    it('rejects .ps1 extension on Windows', () => {
      expect(() => findServerBinary(EXT, 'C:\\tools\\server.ps1', true, 'win32', 'x64')).toThrow(
        /disallowed extension ".ps1"/,
      );
    });

    it('resolves path traversal components to absolute path without error', () => {
      // path.resolve normalizes traversal; the result must be absolute
      const result = findServerBinary(EXT, '/usr/local/../../bin/rlsp-yaml', true, 'linux', 'x64');
      expect(path.isAbsolute(result)).toBe(true);
      expect(result).toBe(path.resolve('/usr/local/../../bin/rlsp-yaml'));
    });

    it('does not apply security checks to bundled binary path', () => {
      // Bundled path should always succeed regardless of workspace trust
      expect(() => findServerBinary(EXT, '', false, 'linux', 'x64')).not.toThrow();
    });
  });
});
