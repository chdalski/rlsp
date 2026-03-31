import { window, type OutputChannel } from 'vscode';
import { type LanguageClient } from 'vscode-languageclient/node';
import { type StatusBar } from './status.js';

export function makeRestartServer(
  getClient: () => LanguageClient | undefined,
  setClient: (client: LanguageClient | undefined) => void,
  startClient: () => Promise<LanguageClient>,
  statusBar: StatusBar,
): () => Promise<void> {
  return async () => {
    const current = getClient();
    if (current !== undefined) {
      statusBar.update('stopped');
      await current.stop();
      setClient(undefined);
    }
    statusBar.update('starting');
    const next = await startClient();
    setClient(next);
  };
}

export function makeShowOutput(outputChannel: OutputChannel): () => void {
  return () => {
    outputChannel.show();
  };
}

export function makeShowVersion(version: string): () => void {
  return () => {
    void window.showInformationMessage(`rlsp-yaml version: ${version}`);
  };
}
