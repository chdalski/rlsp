import { readFileSync } from 'fs';
import * as path from 'path';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { workspace } from 'vscode';
import { getConfig } from './config.js';

vi.mock('vscode', () => ({
  workspace: {
    getConfiguration: vi.fn(),
  },
}));

type InspectResult =
  | {
      defaultValue?: number;
      globalValue?: number;
      workspaceValue?: number;
      workspaceFolderValue?: number;
    }
  | undefined;

function makeConfigStub(inspectReturn: InspectResult) {
  return {
    get: vi.fn().mockReturnValue(undefined),
    inspect: vi.fn().mockReturnValue(inspectReturn),
  };
}

beforeEach(() => {
  vi.resetAllMocks();
});

describe('getConfig — formatPrintWidth field presence', () => {
  it('omitted when all inspect scopes are undefined', () => {
    vi.mocked(workspace.getConfiguration).mockReturnValue(
      makeConfigStub({ defaultValue: 80 }) as never,
    );
    const result = getConfig();
    expect('formatPrintWidth' in result).toBe(false);
  });

  it('omitted when inspect returns undefined (key not registered)', () => {
    vi.mocked(workspace.getConfiguration).mockReturnValue(makeConfigStub(undefined) as never);
    const result = getConfig();
    expect('formatPrintWidth' in result).toBe(false);
  });

  it('included when globalValue is 80', () => {
    vi.mocked(workspace.getConfiguration).mockReturnValue(
      makeConfigStub({ globalValue: 80 }) as never,
    );
    const result = getConfig();
    expect('formatPrintWidth' in result).toBe(true);
    expect(result.formatPrintWidth).toBe(80);
  });

  it('included when globalValue is 100', () => {
    vi.mocked(workspace.getConfiguration).mockReturnValue(
      makeConfigStub({ globalValue: 100 }) as never,
    );
    const result = getConfig();
    expect(result.formatPrintWidth).toBe(100);
  });

  it('included when workspaceValue is set', () => {
    vi.mocked(workspace.getConfiguration).mockReturnValue(
      makeConfigStub({ workspaceValue: 120 }) as never,
    );
    const result = getConfig();
    expect(result.formatPrintWidth).toBe(120);
  });

  it('included when workspaceFolderValue is set', () => {
    vi.mocked(workspace.getConfiguration).mockReturnValue(
      makeConfigStub({ workspaceFolderValue: 60 }) as never,
    );
    const result = getConfig();
    expect(result.formatPrintWidth).toBe(60);
  });

  it('workspaceFolderValue takes precedence when multiple scopes are set', () => {
    vi.mocked(workspace.getConfiguration).mockReturnValue(
      makeConfigStub({
        globalValue: 80,
        workspaceValue: 100,
        workspaceFolderValue: 120,
      }) as never,
    );
    const result = getConfig();
    expect(result.formatPrintWidth).toBe(120);
  });

  it('included when workspaceValue is explicitly 0 (falsy boundary)', () => {
    vi.mocked(workspace.getConfiguration).mockReturnValue(
      makeConfigStub({ workspaceValue: 0 }) as never,
    );
    const result = getConfig();
    expect('formatPrintWidth' in result).toBe(true);
    expect(result.formatPrintWidth).toBe(0);
  });
});

describe('getConfig — other fields unaffected', () => {
  it('non-inspect fields retain values when formatPrintWidth is omitted', () => {
    const stub = makeConfigStub({ defaultValue: 80 });
    stub.get.mockImplementation((key: string) => {
      if (key === 'keyOrdering') return true;
      return undefined;
    });
    vi.mocked(workspace.getConfiguration).mockReturnValue(stub as never);
    const result = getConfig();
    expect(result.keyOrdering).toBe(true);
    expect('formatPrintWidth' in result).toBe(false);
  });

  it('fully-set config has correct shape when formatPrintWidth is present', () => {
    const stub = makeConfigStub({ globalValue: 80 });
    stub.get.mockImplementation((key: string) => {
      const defaults: Record<string, unknown> = {
        customTags: [],
        keyOrdering: false,
        kubernetesVersion: 'master',
        schemaStore: true,
        formatValidation: true,
        formatSingleQuote: false,
        formatPreserveQuotes: false,
        formatBracketSpacing: true,
        httpProxy: '',
        colorDecorators: true,
        schemas: {},
        yamlVersion: '1.2',
        validate: true,
        flowStyle: 'warning',
        formatEnforceBlockStyle: false,
        duplicateKeys: 'error',
        formatRemoveDuplicateKeys: false,
        formatIndentSequences: true,
        formatEnable: true,
      };
      return defaults[key];
    });
    vi.mocked(workspace.getConfiguration).mockReturnValue(stub as never);
    const result = getConfig();
    expect(result.formatPrintWidth).toBe(80);
    expect(result.keyOrdering).toBe(false);
    expect(result.formatEnable).toBe(true);
  });
});

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

// The `contributes.configuration.properties` of the manifest, keyed without
// the `rlsp-yaml.` prefix. Every read is shape-checked so a malformed
// manifest fails with a message that names the field.
function readDeclaredSettings(): Map<string, Record<string, unknown>> {
  const manifest: unknown = JSON.parse(
    readFileSync(path.join(__dirname, '..', 'package.json'), 'utf8'),
  );
  const contributes = isRecord(manifest) ? manifest['contributes'] : undefined;
  const configuration = isRecord(contributes) ? contributes['configuration'] : undefined;
  const properties = isRecord(configuration) ? configuration['properties'] : undefined;
  if (!isRecord(properties)) {
    throw new Error('package.json is missing "contributes.configuration.properties"');
  }
  const declared = new Map<string, Record<string, unknown>>();
  for (const [key, setting] of Object.entries(properties)) {
    if (!key.startsWith('rlsp-yaml.') || !isRecord(setting)) {
      throw new Error(`package.json setting "${key}" is not an rlsp-yaml setting object`);
    }
    declared.set(key.slice('rlsp-yaml.'.length), setting);
  }
  return declared;
}

// Answers every `get(key, fallback)` with the fallback, like VS Code does for
// a setting the user has not set, and records the keys that were asked for.
function makeDefaultingStub() {
  const asked: string[] = [];
  return {
    asked,
    get: vi.fn((key: string, fallback?: unknown) => {
      asked.push(key);
      return fallback;
    }),
    inspect: vi.fn().mockReturnValue({ defaultValue: 80 }),
  };
}

describe('getConfig — manifest defaults', () => {
  it('sends every declared setting except server.path and formatPrintWidth with its manifest default when nothing is set', () => {
    const stub = makeDefaultingStub();
    vi.mocked(workspace.getConfiguration).mockReturnValue(stub as never);
    const result: Record<string, unknown> = { ...getConfig() };

    const declared = readDeclaredSettings();
    declared.delete('server.path');
    expect('formatPrintWidth' in result).toBe(false);
    expect([...Object.keys(result), 'formatPrintWidth'].sort()).toEqual(
      [...declared.keys()].sort(),
    );
    declared.delete('formatPrintWidth');
    for (const [key, setting] of declared) {
      expect(result[key], key).toEqual(setting['default']);
    }
  });
});

describe('getConfig — hover, completion and maxItemsComputed', () => {
  it.each([
    ['hover', false],
    ['completion', false],
    ['maxItemsComputed', 0],
    ['maxItemsComputed', 1],
  ])('passes the user-set %s value %s through', (key, value) => {
    const stub = makeDefaultingStub();
    stub.get.mockImplementation((asked: string, fallback?: unknown) =>
      asked === key ? value : fallback,
    );
    vi.mocked(workspace.getConfiguration).mockReturnValue(stub as never);
    expect({ ...getConfig() }[key]).toBe(value);
  });

  it.each([
    ['maxItemsComputed', 'abc'],
    ['maxItemsComputed', -1],
    ['maxItemsComputed', 1.5],
    ['hover', 'yes'],
    ['completion', 1],
  ])('passes the wrongly typed %s value %j on for the server to judge', (key, value) => {
    const stub = makeDefaultingStub();
    stub.get.mockImplementation((asked: string, fallback?: unknown) =>
      asked === key ? value : fallback,
    );
    vi.mocked(workspace.getConfiguration).mockReturnValue(stub as never);
    expect({ ...getConfig() }[key]).toBe(value);
  });
});

describe('getConfig — schemas', () => {
  it('passes the string and array forms through unchanged', () => {
    const schemas = {
      'https://a.test/s.json': ['*.yaml', '!skip.yaml'],
      'https://b.test/s.json': '*.yml',
    };
    const stub = makeDefaultingStub();
    stub.get.mockImplementation((key: string, fallback?: unknown) =>
      key === 'schemas' ? schemas : fallback,
    );
    vi.mocked(workspace.getConfiguration).mockReturnValue(stub as never);
    expect(getConfig().schemas).toEqual(schemas);
  });
});
