import * as assert from 'assert';
import * as vscode from 'vscode';
import * as mainModule from '../../main.js';

const EXTENSION_ID = 'chrisski.rlsp-yaml';

suite('extension manifest', () => {
  test('extension is present in VS Code', () => {
    const ext = vscode.extensions.getExtension(EXTENSION_ID);
    assert.notStrictEqual(ext, undefined);
  });

  test('extension identifier matches package.json', () => {
    const ext = vscode.extensions.getExtension(EXTENSION_ID);
    assert.strictEqual(ext?.id, EXTENSION_ID);
  });
});

suite('module shape', () => {
  test('activate export is a function', () => {
    assert.strictEqual(typeof mainModule.activate, 'function');
  });

  test('deactivate export is a function', () => {
    assert.strictEqual(typeof mainModule.deactivate, 'function');
  });
});

suite('activation failure (no binary)', () => {
  test('activate() rejects when no server binary is present', async () => {
    const ext = vscode.extensions.getExtension(EXTENSION_ID);
    assert.ok(ext !== undefined);
    await assert.rejects(async () => {
      await ext.activate();
    });
  });
});
