import * as path from 'path';

// Supported platform/arch combinations and their Rust target triples.
const PLATFORM_TARGETS: Readonly<Record<string, Record<string, string>>> = {
  linux: {
    x64: 'x86_64-unknown-linux-gnu',
    arm64: 'aarch64-unknown-linux-gnu',
  },
  darwin: {
    x64: 'x86_64-apple-darwin',
    arm64: 'aarch64-apple-darwin',
  },
  win32: {
    x64: 'x86_64-pc-windows-msvc',
    arm64: 'aarch64-pc-windows-msvc',
  },
};

const WINDOWS_BLOCKED_EXTENSIONS = new Set(['.cmd', '.bat', '.ps1', '.vbs', '.wsf']);

/**
 * Resolve the rlsp-yaml server binary path.
 *
 * @param extensionPath - Absolute path to the extension install directory.
 * @param serverPath    - Value of `rlsp-yaml.server.path` (empty string = unset).
 * @param workspaceTrusted - Whether the current workspace is trusted.
 * @param platform      - Override for `process.platform` (for testing).
 * @param arch          - Override for `process.arch` (for testing).
 */
export function findServerBinary(
  extensionPath: string,
  serverPath: string,
  workspaceTrusted: boolean,
  platform: string = process.platform,
  arch: string = process.arch,
): string {
  const trimmed = serverPath.trim();

  if (trimmed.length > 0 && workspaceTrusted) {
    // Reject UNC paths before resolving — path.resolve() normalizes them differently per OS.
    // Both \\ (Windows SMB) and // (POSIX double-slash) can trigger NTLM credential capture.
    if (trimmed.startsWith('\\\\') || trimmed.startsWith('//')) {
      throw new Error(
        `rlsp-yaml: server path "${trimmed}" is a UNC path and cannot be used for security reasons. ` +
          `Set rlsp-yaml.server.path to a local binary path.`,
      );
    }

    const resolved = path.resolve(trimmed);

    // Reject script file extensions on Windows — they invoke cmd.exe/powershell with shell semantics.
    if (platform === 'win32') {
      const ext = path.extname(resolved).toLowerCase();
      if (WINDOWS_BLOCKED_EXTENSIONS.has(ext)) {
        throw new Error(
          `rlsp-yaml: server path "${trimmed}" has a disallowed extension "${ext}". ` +
            `Only .exe binaries are accepted on Windows.`,
        );
      }
    }

    return resolved;
  }

  return bundledBinaryPath(extensionPath, platform, arch);
}

function bundledBinaryPath(extensionPath: string, platform: string, arch: string): string {
  const targets = PLATFORM_TARGETS[platform];
  if (targets === undefined) {
    throw new Error(
      `rlsp-yaml: unsupported platform "${platform}". ` +
        `Install rlsp-yaml manually and set rlsp-yaml.server.path.`,
    );
  }

  const target = targets[arch];
  if (target === undefined) {
    throw new Error(
      `rlsp-yaml: unsupported architecture "${arch}" on platform "${platform}". ` +
        `Install rlsp-yaml manually and set rlsp-yaml.server.path.`,
    );
  }

  const binaryName = platform === 'win32' ? 'rlsp-yaml.exe' : 'rlsp-yaml';
  return path.join(extensionPath, 'server', target, binaryName);
}
