import { beforeEach, describe, expect, it, vi } from 'vitest';
import { type LogOutputChannel } from 'vscode';
import { type LanguageClientOptions } from 'vscode-languageclient/node';
import { createLanguageClient } from './client.js';
import { getConfig, type ServerSettings } from './config.js';

type ChangeListener = (event: { affectsConfiguration: (section: string) => boolean }) => void;

const { constructed, changeListeners, sendNotification } = vi.hoisted(() => ({
  constructed: [] as unknown[][],
  changeListeners: [] as unknown[],
  sendNotification: vi.fn(),
}));

vi.mock('vscode', () => ({
  workspace: {
    onDidChangeConfiguration: (listener: unknown) => {
      changeListeners.push(listener);
      return { dispose: vi.fn() };
    },
  },
}));

// The fake records the constructor arguments and exposes the standalone
// sendNotification spy, so assertions never read a method off the typed client
// (unbound-method / no-deprecated).
vi.mock('vscode-languageclient/node', () => ({
  TransportKind: { stdio: 0 },
  LanguageClient: class {
    constructor(...args: unknown[]) {
      constructed.push(args);
    }
    sendNotification = sendNotification;
  },
}));

vi.mock('./config.js', () => ({ getConfig: vi.fn() }));

const flat = { formatSingleQuote: true, customTags: ['!include'] } as unknown as ServerSettings;

function create(): LanguageClientOptions {
  createLanguageClient('/bin/rlsp-yaml', {} as unknown as LogOutputChannel);
  return constructed[0]?.[3] as LanguageClientOptions;
}

function fireChange(section: string): void {
  const listener = changeListeners[0] as ChangeListener;
  listener({ affectsConfiguration: (s) => s === section });
}

beforeEach(() => {
  constructed.length = 0;
  changeListeners.length = 0;
  sendNotification.mockClear();
  vi.mocked(getConfig).mockReturnValue(flat);
});

describe('createLanguageClient options', () => {
  it('does not set synchronize, which would send the settings nested', () => {
    expect(create()).not.toHaveProperty('synchronize');
  });

  it('passes the flat getConfig() result as initializationOptions', () => {
    const options = create();

    expect(options.initializationOptions).toBe(flat);
    expect(options.initializationOptions).not.toHaveProperty('rlsp-yaml');
  });
});

describe('createLanguageClient settings listener', () => {
  it('registers exactly one configuration listener', () => {
    create();

    expect(changeListeners).toHaveLength(1);
  });

  it('sends one flat didChangeConfiguration when an rlsp-yaml setting changes', () => {
    create();

    fireChange('rlsp-yaml');

    expect(sendNotification).toHaveBeenCalledTimes(1);
    expect(sendNotification).toHaveBeenCalledWith('workspace/didChangeConfiguration', {
      settings: flat,
    });
    expect(sendNotification.mock.calls[0]?.[1]).not.toHaveProperty('settings.rlsp-yaml');
  });

  it.each(['editor', 'files', 'yaml'])('sends nothing when the %s section changes', (section) => {
    create();

    fireChange(section);

    expect(sendNotification).not.toHaveBeenCalled();
  });

  it('reads the settings at change time, not at creation time', () => {
    create();
    const changed = { formatSingleQuote: false } as unknown as ServerSettings;
    vi.mocked(getConfig).mockReturnValue(changed);

    fireChange('rlsp-yaml');

    expect(sendNotification).toHaveBeenCalledWith('workspace/didChangeConfiguration', {
      settings: changed,
    });
  });

  it('sends one notification per change event', () => {
    create();

    fireChange('rlsp-yaml');
    fireChange('rlsp-yaml');

    expect(sendNotification).toHaveBeenCalledTimes(2);
  });
});
