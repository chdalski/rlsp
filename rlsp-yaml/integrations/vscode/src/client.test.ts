import { beforeEach, describe, expect, it, vi } from 'vitest';
import { type LogOutputChannel } from 'vscode';
import {
  LanguageClient,
  State,
  TransportKind,
  type LanguageClientOptions,
} from 'vscode-languageclient/node';
import { createLanguageClient } from './client.js';
import { getConfig, type ServerSettings } from './config.js';

type ChangeListener = (event: { affectsConfiguration: (section: string) => boolean }) => void;

const { constructed, changeListeners, sendNotification, baseStop, warn } = vi.hoisted(() => ({
  constructed: [] as unknown[][],
  changeListeners: new Set<unknown>(),
  sendNotification: vi.fn(),
  baseStop: vi.fn(),
  warn: vi.fn(),
}));

vi.mock('vscode', () => ({
  workspace: {
    onDidChangeConfiguration: (listener: unknown) => {
      changeListeners.add(listener);
      return {
        dispose: () => {
          changeListeners.delete(listener);
        },
      };
    },
  },
}));

// The fake mirrors the parts of the library the subclass relies on: stop() and
// sendNotification() are prototype methods (so the subclass override is really
// exercised), dispose() goes through this.stop() like the library's, and the
// base stop() behaviour is steered per test through the standalone baseStop
// spy. Assertions use standalone spies so they never read a method off the
// typed client (unbound-method / no-deprecated).
vi.mock('vscode-languageclient/node', () => ({
  TransportKind: { stdio: 0 },
  State: { Stopped: 1, Running: 2, Starting: 3, StartFailed: 4 },
  LanguageClient: class {
    state = 2;
    stateListeners = new Set<(event: { oldState: number; newState: number }) => void>();
    constructor(...args: unknown[]) {
      constructed.push(args);
    }
    onDidChangeState(listener: (event: { oldState: number; newState: number }) => void) {
      this.stateListeners.add(listener);
      return {
        dispose: () => {
          this.stateListeners.delete(listener);
        },
      };
    }
    sendNotification(method: string, params: unknown): unknown {
      return sendNotification.call(this, method, params);
    }
    stop(timeout?: number): unknown {
      return baseStop.call(this, timeout);
    }
    dispose(timeout?: number): unknown {
      return this.stop(timeout);
    }
  },
}));

vi.mock('./config.js', () => ({ getConfig: vi.fn() }));

const flat = { formatSingleQuote: true, customTags: ['!include'] } as unknown as ServerSettings;
const outputChannel = { warn } as unknown as LogOutputChannel;

function create(): LanguageClient {
  return createLanguageClient('/bin/rlsp-yaml', outputChannel);
}

function lastArgs(): unknown[] {
  return constructed.at(-1) ?? [];
}

function lastOptions(): LanguageClientOptions {
  return lastArgs()[3] as LanguageClientOptions;
}

// Like the library, fires a state change event only for a real change.
function setState(client: LanguageClient, state: State): void {
  const fake = client as unknown as {
    state: State;
    stateListeners: Set<(event: { oldState: State; newState: State }) => void>;
  };
  const oldState = fake.state;
  fake.state = state;
  if (oldState !== state) {
    [...fake.stateListeners].forEach((listener) => {
      listener({ oldState, newState: state });
    });
  }
}

// The base stop() of a client that ends in `endState`, optionally rejecting.
function baseStopEnding(endState: State | undefined, error?: Error): void {
  baseStop.mockImplementation(function (this: LanguageClient) {
    if (endState !== undefined) {
      setState(this, endState);
    }
    return error === undefined ? Promise.resolve() : Promise.reject(error);
  });
}

function fireChange(section: string): void {
  [...changeListeners].forEach((listener) => {
    (listener as ChangeListener)({ affectsConfiguration: (s) => s === section });
  });
}

function notificationCount(): number {
  fireChange('rlsp-yaml');
  const sent = sendNotification.mock.calls.length;
  sendNotification.mockClear();
  return sent;
}

beforeEach(() => {
  constructed.length = 0;
  changeListeners.clear();
  sendNotification.mockReset().mockResolvedValue(undefined);
  baseStop.mockReset();
  baseStopEnding(State.Stopped);
  warn.mockReset();
  vi.mocked(getConfig).mockReset().mockReturnValue(flat);
});

describe('createLanguageClient options', () => {
  it('does not set synchronize, which would send the settings nested', () => {
    create();

    expect(lastOptions()).not.toHaveProperty('synchronize');
  });

  it('initializationOptions is a function returning the flat getConfig() result', () => {
    create();
    const initializationOptions = lastOptions().initializationOptions as () => unknown;

    expect(typeof initializationOptions).toBe('function');
    expect(initializationOptions()).toBe(flat);
    expect(initializationOptions()).not.toHaveProperty('rlsp-yaml');
  });

  it('initializationOptions reads the settings at each call, not at creation time', () => {
    create();
    const initializationOptions = lastOptions().initializationOptions as () => unknown;
    expect(getConfig).not.toHaveBeenCalled();
    const changed = { formatSingleQuote: false } as unknown as ServerSettings;
    vi.mocked(getConfig).mockReturnValue(changed);

    expect(initializationOptions()).toBe(changed);
  });

  it('constructs a LanguageClient with id, name, server options and client options', () => {
    const client = create();

    expect(client).toBeInstanceOf(LanguageClient);
    const [id, name, serverOptions, clientOptions] = lastArgs();
    expect(id).toBe('rlsp-yaml');
    expect(name).toBe('rlsp-yaml');
    expect(serverOptions).toEqual({
      command: '/bin/rlsp-yaml',
      args: [],
      transport: TransportKind.stdio,
    });
    expect(clientOptions).toMatchObject({
      documentSelector: [{ language: 'yaml' }],
      outputChannel,
    });
  });
});

describe('createLanguageClient settings listener', () => {
  it('registers exactly one configuration listener', () => {
    create();

    expect(changeListeners.size).toBe(1);
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

describe('createLanguageClient listener lifecycle', () => {
  it.each([
    ['running', State.Running],
    ['never started', State.Stopped],
  ])('disposes the listener when stop() resolves for a %s client', async (_label, startState) => {
    const client = create();
    setState(client, startState);

    await client.stop();

    expect(notificationCount()).toBe(0);
  });

  it('keeps the listener while stop() is still pending and disposes it once it settles', async () => {
    const client = create();
    let finish = (): void => undefined;
    baseStop.mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          finish = resolve;
        }),
    );

    const stopping = client.stop();
    expect(notificationCount()).toBe(1);
    setState(client, State.Stopped);
    finish();
    await stopping;

    expect(notificationCount()).toBe(0);
  });

  it('disposes the listener and rethrows when stop() rejects and the client ends stopped', async () => {
    const client = create();
    const timeout = new Error('Stopping the server timed out');
    baseStopEnding(State.Stopped, timeout);

    await expect(client.stop()).rejects.toBe(timeout);

    expect(notificationCount()).toBe(0);
  });

  it.each([
    ['starting', State.Starting],
    ['running', State.Running],
  ])(
    'keeps the listener and rethrows when stop() rejects with the client %s',
    async (_label, state) => {
      const client = create();
      const refused = new Error("Client is not running and can't be stopped");
      setState(client, state);
      baseStopEnding(undefined, refused);

      await expect(client.stop()).rejects.toBe(refused);

      expect(notificationCount()).toBe(1);
    },
  );

  it('disposes the listener when the client enters StartFailed', () => {
    const client = create();
    setState(client, State.Starting);

    setState(client, State.StartFailed);

    expect(notificationCount()).toBe(0);
  });

  it.each([
    ['starting', State.Running, State.Starting],
    ['running', State.Starting, State.Running],
    ['stopped', State.Running, State.Stopped],
  ])('keeps the listener on a transition to %s', (_label, from, to) => {
    const client = create();
    setState(client, from);

    setState(client, to);

    expect(notificationCount()).toBe(1);
  });

  it('disposes the listener when a rejected stop() is followed by StartFailed', async () => {
    const client = create();
    const refused = new Error("Client is not running and can't be stopped");
    setState(client, State.Starting);
    baseStopEnding(undefined, refused);

    await expect(client.stop()).rejects.toBe(refused);
    expect(notificationCount()).toBe(1);
    setState(client, State.StartFailed);

    expect(notificationCount()).toBe(0);
  });

  it('disposes the listener when StartFailed precedes a rejected stop()', async () => {
    const client = create();
    const refused = new Error("Client is not running and can't be stopped");
    setState(client, State.Starting);
    setState(client, State.StartFailed);
    baseStopEnding(undefined, refused);

    await expect(client.stop()).rejects.toBe(refused);

    expect(notificationCount()).toBe(0);
  });

  it('survives disposal from both StartFailed and stop()', async () => {
    const client = create();
    setState(client, State.StartFailed);
    setState(client, State.Stopped);

    await client.stop();
    await client.dispose();

    expect(notificationCount()).toBe(0);
  });

  it('disposes the listener when a later stop() succeeds after a rejected one', async () => {
    const client = create();
    setState(client, State.Starting);
    baseStopEnding(undefined, new Error('not running'));
    await expect(client.stop()).rejects.toThrow('not running');
    setState(client, State.Running);
    baseStopEnding(State.Stopped);

    await client.stop();

    expect(notificationCount()).toBe(0);
  });

  it('keeps the listener across a crash restart that never calls stop()', () => {
    const client = create();

    [State.Stopped, State.Starting, State.Running].forEach((state) => {
      setState(client, state);
    });

    expect(notificationCount()).toBe(1);
    expect(baseStop).not.toHaveBeenCalled();
  });

  it('forwards the timeout to the base stop() from stop() and from dispose()', async () => {
    const client = create();

    await client.stop(5000);
    await client.dispose(3000);

    expect(baseStop.mock.calls.map((call): unknown => call[0])).toEqual([5000, 3000]);
  });

  it('settles two concurrent stop() calls and disposes the listener', async () => {
    const client = create();
    let finish = (): void => undefined;
    const shared = new Promise<void>((resolve) => {
      finish = resolve;
    });
    baseStop.mockImplementation((): Promise<void> => shared);

    const both = Promise.all([client.stop(), client.stop()]);
    setState(client, State.Stopped);
    finish();
    await both;

    expect(notificationCount()).toBe(0);
  });

  it('resolves a second sequential stop() and keeps the listener disposed', async () => {
    const client = create();
    await client.stop();

    await expect(client.stop()).resolves.toBeUndefined();

    expect(notificationCount()).toBe(0);
  });

  it('disposes the listener when the client is disposed', async () => {
    const client = create();

    await client.dispose();

    expect(notificationCount()).toBe(0);
  });

  it('stopping one client leaves the listener of another client in place', async () => {
    const first = create();
    const second = create();
    expect(notificationCount()).toBe(2);

    await first.stop();
    fireChange('rlsp-yaml');

    expect(sendNotification).toHaveBeenCalledTimes(1);
    expect(sendNotification.mock.contexts[0]).toBe(second);
  });

  it('a StartFailed client leaves the listener of another client in place', () => {
    const first = create();
    const second = create();

    setState(first, State.StartFailed);
    fireChange('rlsp-yaml');

    expect(sendNotification).toHaveBeenCalledTimes(1);
    expect(sendNotification.mock.contexts[0]).toBe(second);
  });
});

describe('createLanguageClient failing notifications', () => {
  async function failWith(reason: unknown): Promise<void> {
    sendNotification.mockRejectedValue(reason);
    create();
    fireChange('rlsp-yaml');
    await vi.waitFor(() => {
      expect(warn).toHaveBeenCalled();
    });
  }

  it('writes a rejected notification to the output channel', async () => {
    await failWith(new Error('Client is not running'));

    expect(warn).toHaveBeenCalledTimes(1);
    expect(warn).toHaveBeenCalledWith(
      'Failed to send workspace/didChangeConfiguration: Client is not running',
    );
  });

  it('logs a single string and never the settings or the error payload', async () => {
    const secret = 'SECRET-SENTINEL';
    vi.mocked(getConfig).mockReturnValue({
      httpProxy: `http://user:${secret}@proxy:8080`,
      schemas: { '*.yaml': `https://example.test/?token=${secret}` },
    } as unknown as ServerSettings);
    const error = Object.assign(new Error('Client is not running', { cause: { secret } }), {
      data: { settings: { secret } },
    });

    await failWith(error);

    expect(warn.mock.calls.every((call) => call.length === 1 && typeof call[0] === 'string')).toBe(
      true,
    );
    expect(JSON.stringify(warn.mock.calls)).not.toContain(secret);
  });

  it.each([
    ['a string', 'SECRET-SENTINEL'],
    ['undefined', undefined],
    ['an object', { secret: 'SECRET-SENTINEL' }],
  ])('logs a fixed reason for a rejection with %s', async (_label, reason) => {
    await failWith(reason);

    expect(warn).toHaveBeenCalledWith(
      'Failed to send workspace/didChangeConfiguration: unknown error',
    );
  });

  it('logs nothing when the notification succeeds', async () => {
    create();

    fireChange('rlsp-yaml');
    await Promise.resolve();

    expect(warn).not.toHaveBeenCalled();
  });

  it('keeps sending after a failed notification', async () => {
    sendNotification.mockRejectedValueOnce(new Error('Client is not running'));
    create();

    fireChange('rlsp-yaml');
    await vi.waitFor(() => {
      expect(warn).toHaveBeenCalledTimes(1);
    });
    fireChange('rlsp-yaml');

    expect(sendNotification).toHaveBeenCalledTimes(2);
  });
});
