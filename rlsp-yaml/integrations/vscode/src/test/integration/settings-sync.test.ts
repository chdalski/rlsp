import * as assert from 'assert';
import { existsSync } from 'fs';
import * as path from 'path';
import {
  commands,
  ConfigurationTarget,
  extensions,
  workspace,
  type TextDocument,
  type TextEdit,
} from 'vscode';

const EXTENSION_ID = 'chrisski.rlsp-yaml';
const POLL_INTERVAL_MS = 100;
const POLL_TIMEOUT_MS = 10_000;
// A stale nested settings payload from the client library lands shortly after
// the flat one; the formatting must still hold once it had time to arrive.
const SETTLE_MS = 500;

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function serverBinaryPath(): string {
  const ext = extensions.getExtension(EXTENSION_ID);
  assert.ok(ext !== undefined);
  return path.join(ext.extensionPath, 'server', 'x86_64-unknown-linux-gnu', 'rlsp-yaml');
}

async function formatDocument(doc: TextDocument): Promise<string> {
  const edits = await commands.executeCommand<TextEdit[] | undefined>(
    'vscode.executeFormatDocumentProvider',
    doc.uri,
    { tabSize: 2, insertSpaces: true },
  );
  // The server answers with minimal edits, so apply them to the text.
  return [...(edits ?? [])]
    .sort((a, b) => doc.offsetAt(b.range.start) - doc.offsetAt(a.range.start))
    .reduce(
      (text, edit) =>
        text.slice(0, doc.offsetAt(edit.range.start)) +
        edit.newText +
        text.slice(doc.offsetAt(edit.range.end)),
      doc.getText(),
    );
}

// Turns formatSingleQuote on and waits until the server formats with it, then
// gives a stale nested payload time to arrive and checks the setting still holds.
async function enableSingleQuoteAndExpectItToStick(doc: TextDocument): Promise<void> {
  await workspace
    .getConfiguration('rlsp-yaml')
    .update('formatSingleQuote', true, ConfigurationTarget.Global);

  const deadline = Date.now() + POLL_TIMEOUT_MS;
  while ((await formatDocument(doc)) !== "key: 'v'\n" && Date.now() < deadline) {
    await sleep(POLL_INTERVAL_MS);
  }
  await sleep(SETTLE_MS);
  assert.strictEqual(await formatDocument(doc), "key: 'v'\n", 'formatSingleQuote stays in effect');
}

suite('settings sync (binary present)', () => {
  teardown(async () => {
    await workspace
      .getConfiguration('rlsp-yaml')
      .update('formatSingleQuote', undefined, ConfigurationTarget.Global);
  });

  test('a changed formatSingleQuote reaches the running server', async function () {
    if (!existsSync(serverBinaryPath())) {
      this.skip();
    }
    await extensions.getExtension(EXTENSION_ID)?.activate();

    const doc = await workspace.openTextDocument({ language: 'yaml', content: 'key: "v"\n' });

    assert.strictEqual(await formatDocument(doc), 'key: v\n', 'default settings drop the quotes');
    await enableSingleQuoteAndExpectItToStick(doc);
  });

  test('a changed formatSingleQuote reaches the server after Restart Server', async function () {
    if (!existsSync(serverBinaryPath())) {
      this.skip();
    }
    await extensions.getExtension(EXTENSION_ID)?.activate();
    await commands.executeCommand('rlsp-yaml.restartServer');

    const doc = await workspace.openTextDocument({ language: 'yaml', content: 'key: "v"\n' });

    await enableSingleQuoteAndExpectItToStick(doc);
  });
});
