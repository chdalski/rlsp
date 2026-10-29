import { workspace, type LogOutputChannel } from 'vscode';
import {
  LanguageClient,
  type LanguageClientOptions,
  type ServerOptions,
  TransportKind,
} from 'vscode-languageclient/node';
import { getConfig } from './config.js';

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
    initializationOptions: getConfig(),
    outputChannel,
  };

  const client = new LanguageClient('rlsp-yaml', 'rlsp-yaml', serverOptions, clientOptions);

  // The extension sends the flat settings object itself. The client library's
  // `synchronize.configurationSection` would send them nested under
  // `rlsp-yaml`, which the server reads as all defaults and so overrides these.
  workspace.onDidChangeConfiguration((event) => {
    if (event.affectsConfiguration('rlsp-yaml')) {
      void client.sendNotification('workspace/didChangeConfiguration', {
        settings: getConfig(),
      });
    }
  });

  return client;
}
