import { workspace, type OutputChannel } from 'vscode';
import {
  LanguageClient,
  type LanguageClientOptions,
  type ServerOptions,
  TransportKind,
} from 'vscode-languageclient/node';
import { getConfig } from './config.js';

export function createLanguageClient(
  serverBinaryPath: string,
  outputChannel: OutputChannel,
): LanguageClient {
  const serverOptions: ServerOptions = {
    command: serverBinaryPath,
    args: [],
    transport: TransportKind.stdio,
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ language: 'yaml' }],
    synchronize: {
      configurationSection: 'rlsp-yaml',
    },
    initializationOptions: getConfig(),
    outputChannel,
  };

  const client = new LanguageClient('rlsp-yaml', 'rlsp-yaml', serverOptions, clientOptions);

  workspace.onDidChangeConfiguration((event) => {
    if (event.affectsConfiguration('rlsp-yaml')) {
      void client.sendNotification('workspace/didChangeConfiguration', {
        settings: getConfig(),
      });
    }
  });

  return client;
}
