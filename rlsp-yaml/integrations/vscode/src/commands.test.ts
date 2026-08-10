import { beforeEach, describe, expect, it, vi } from 'vitest';
import { window, type LogOutputChannel } from 'vscode';
import { type LanguageClient } from 'vscode-languageclient/node';
import { makeRestartServer, makeShowOutput, makeShowVersion } from './commands.js';
import { type StatusBar } from './status.js';

vi.mock('vscode', () => ({
  window: {
    showInformationMessage: vi.fn(),
  },
}));

// Structural fakes built from standalone spies: makeShowOutput only ever
// calls .show() on the channel it's given, so proving that call happens
// doesn't require a real LogOutputChannel (trace/debug/logLevel/etc. are
// untouched). Asserting on the standalone spy variable -- rather than on a
// `.show`/`.stop`/`.update` property read off the typed fake -- also avoids
// @typescript-eslint/unbound-method and no-deprecated, both of which trigger
// on referencing the real (overloaded, partly-deprecated) vscode method
// signatures without calling them.
function makeOutputChannelFake(show: () => void): LogOutputChannel {
  return { show } as unknown as LogOutputChannel;
}

function makeClientFake(stop: () => Promise<void>): LanguageClient {
  return { stop } as unknown as LanguageClient;
}

function makeStatusBarFake(update: (state: string) => void): StatusBar {
  return { update } as unknown as StatusBar;
}

beforeEach(() => {
  vi.resetAllMocks();
});

describe('makeShowOutput', () => {
  it('calls show() on the provided channel when invoked', () => {
    const show = vi.fn();
    const channel = makeOutputChannelFake(show);

    makeShowOutput(channel)();

    expect(show).toHaveBeenCalledOnce();
  });
});

describe('makeShowVersion', () => {
  it('shows an information message containing the given version', () => {
    makeShowVersion('1.2.3')();

    expect(window.showInformationMessage).toHaveBeenCalledWith(expect.stringContaining('1.2.3'));
  });
});

describe('makeRestartServer', () => {
  it('stops the current client, updates status through stop→start, and swaps in the new client when a client is already running', async () => {
    const calls: string[] = [];
    const stop = vi.fn().mockImplementation(() => {
      calls.push('stop');
      return Promise.resolve();
    });
    const update = vi.fn().mockImplementation((state: unknown) => {
      calls.push(`update:${String(state)}`);
    });
    const current = makeClientFake(stop);
    const next = makeClientFake(vi.fn());
    const getClient = vi.fn().mockReturnValue(current);
    const setClient = vi.fn().mockImplementation((client: unknown) => {
      calls.push(client === undefined ? 'setClient:undefined' : 'setClient:next');
    });
    const startClient = vi.fn().mockResolvedValue(next);
    const statusBar = makeStatusBarFake(update);

    await makeRestartServer(getClient, setClient, startClient, statusBar)();

    expect(calls).toEqual([
      'update:stopped',
      'stop',
      'setClient:undefined',
      'update:starting',
      'setClient:next',
    ]);
    expect(setClient).toHaveBeenLastCalledWith(next);
  });

  it('skips the stop phase and starts fresh when no client is currently running', async () => {
    const next = makeClientFake(vi.fn());
    const getClient = vi.fn().mockReturnValue(undefined);
    const setClient = vi.fn();
    const startClient = vi.fn().mockResolvedValue(next);
    const update = vi.fn();
    const statusBar = makeStatusBarFake(update);

    await makeRestartServer(getClient, setClient, startClient, statusBar)();

    expect(update).toHaveBeenCalledExactlyOnceWith('starting');
    expect(setClient).toHaveBeenCalledExactlyOnceWith(next);
  });
});
