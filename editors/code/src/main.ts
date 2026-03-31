import { commands, workspace, window, type ExtensionContext } from 'vscode';
import { LanguageClient, State } from 'vscode-languageclient/node';
import { makeRestartServer, makeShowOutput, makeShowVersion } from './commands.js';
import { createLanguageClient } from './client.js';
import { findServerBinary } from './server.js';
import { StatusBar } from './status.js';

let client: LanguageClient | undefined;

export async function activate(context: ExtensionContext): Promise<void> {
  const outputChannel = window.createOutputChannel('rlsp-yaml');
  context.subscriptions.push(outputChannel);

  const statusBar = new StatusBar();
  context.subscriptions.push(statusBar);

  const startClient = async (): Promise<LanguageClient> => {
    const serverPath = workspace.getConfiguration('rlsp-yaml').get('server.path', '');
    const binaryPath = findServerBinary(context.extensionPath, serverPath, workspace.isTrusted);
    const lc = createLanguageClient(binaryPath, outputChannel);

    lc.onDidChangeState((event) => {
      if (event.newState === State.Starting) {
        statusBar.update('starting');
      } else if (event.newState === State.Running) {
        statusBar.update('running');
      } else {
        statusBar.update('stopped');
      }
    });

    statusBar.update('starting');
    await lc.start();
    return lc;
  };

  client = await startClient();
  context.subscriptions.push(client);

  context.subscriptions.push(
    commands.registerCommand(
      'rlsp-yaml.restartServer',
      makeRestartServer(
        () => client,
        (lc) => {
          client = lc;
        },
        startClient,
        statusBar,
      ),
    ),
    commands.registerCommand('rlsp-yaml.showOutput', makeShowOutput(outputChannel)),
    commands.registerCommand('rlsp-yaml.showVersion', makeShowVersion(packageVersion(context))),
  );

  // Restart with the user-supplied path when workspace trust is granted.
  context.subscriptions.push(
    workspace.onDidGrantWorkspaceTrust(() => {
      void makeRestartServer(
        () => client,
        (lc) => {
          client = lc;
        },
        startClient,
        statusBar,
      )();
    }),
  );
}

export async function deactivate(): Promise<void> {
  if (client !== undefined) {
    await client.stop();
    client = undefined;
  }
}

function packageVersion(context: ExtensionContext): string {
  // packageJSON is typed as `any`; access via index to satisfy no-unsafe-member-access.
  const pkg: unknown = context.extension.packageJSON;
  if (typeof pkg === 'object' && pkg !== null && 'version' in pkg) {
    return String((pkg as { version: unknown }).version);
  }
  return 'unknown';
}
