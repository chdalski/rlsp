import { workspace, window, type ExtensionContext } from 'vscode';
import { type LanguageClient } from 'vscode-languageclient/node';
import { createLanguageClient } from './client.js';
import { findServerBinary } from './server.js';

let client: LanguageClient | undefined;

export async function activate(context: ExtensionContext): Promise<void> {
  const outputChannel = window.createOutputChannel('rlsp-yaml');
  context.subscriptions.push(outputChannel);

  const serverPath = workspace.getConfiguration('rlsp-yaml').get('server.path', '');
  const binaryPath = findServerBinary(context.extensionPath, serverPath, workspace.isTrusted);

  client = createLanguageClient(binaryPath, outputChannel);
  await client.start();
  context.subscriptions.push(client);

  // Restart with the user-supplied path when workspace trust is granted.
  context.subscriptions.push(
    workspace.onDidGrantWorkspaceTrust(() => {
      void restartClient(context);
    }),
  );
}

export async function deactivate(): Promise<void> {
  if (client !== undefined) {
    await client.stop();
    client = undefined;
  }
}

async function restartClient(context: ExtensionContext): Promise<void> {
  if (client !== undefined) {
    await client.stop();
    client = undefined;
  }
  await activate(context);
}
