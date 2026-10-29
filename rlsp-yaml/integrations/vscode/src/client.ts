import { workspace, type Disposable, type LogOutputChannel } from 'vscode';
import {
  LanguageClient,
  type LanguageClientOptions,
  type ServerOptions,
  State,
  TransportKind,
} from 'vscode-languageclient/node';
import { getConfig } from './config.js';

// Owns the listener that forwards settings changes to its server, so the
// listener lives exactly as long as the client. The extension sends the flat
// settings object itself: the client library's `synchronize.configurationSection`
// would send them nested under `rlsp-yaml`, which the server reads as all
// defaults and so overrides these.
class SettingsSyncingClient extends LanguageClient {
  private readonly settingsListener: Disposable;

  constructor(
    serverOptions: ServerOptions,
    clientOptions: LanguageClientOptions,
    outputChannel: LogOutputChannel,
  ) {
    super('rlsp-yaml', 'rlsp-yaml', serverOptions, clientOptions);
    this.settingsListener = workspace.onDidChangeConfiguration((event) => {
      if (!event.affectsConfiguration('rlsp-yaml')) {
        return;
      }
      this.sendNotification('workspace/didChangeConfiguration', {
        settings: getConfig(),
      }).catch((error: unknown) => {
        // Only the message is logged: the settings can carry credentials
        // (proxy userinfo, schema URLs) and must never reach the log.
        const reason = error instanceof Error ? error.message : 'unknown error';
        outputChannel.warn(`Failed to send workspace/didChangeConfiguration: ${reason}`);
      });
    });
    // The library never restarts a client from StartFailed, and nothing
    // stops it from there, so this is the only point to release the listener.
    this.onDidChangeState((event) => {
      if (event.newState === State.StartFailed) {
        this.settingsListener.dispose();
      }
    });
  }

  // A crash restart passes through Stopped and keeps this client alive without
  // calling `stop()`, so the listener is released here, and only once the
  // client has ended up stopped (a timed-out shutdown rejects but still ends
  // stopped).
  override async stop(timeout?: number): Promise<void> {
    try {
      await super.stop(timeout);
    } finally {
      if (this.state === State.Stopped) {
        this.settingsListener.dispose();
      }
    }
  }
}

export function createLanguageClient(
  serverBinaryPath: string,
  outputChannel: LogOutputChannel,
): LanguageClient {
  const serverOptions: ServerOptions = {
    command: serverBinaryPath,
    args: [],
    transport: TransportKind.stdio,
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ language: 'yaml' }],
    // The library calls this at every initialize, so a client it restarts
    // after a crash sends the current settings.
    initializationOptions: getConfig,
    outputChannel,
  };

  return new SettingsSyncingClient(serverOptions, clientOptions, outputChannel);
}
