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
