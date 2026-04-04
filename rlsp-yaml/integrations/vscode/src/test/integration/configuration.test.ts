import * as assert from 'assert';
import { workspace } from 'vscode';

function cfg() {
  return workspace.getConfiguration('rlsp-yaml');
}

suite('configuration defaults', () => {
  test('server.path defaults to empty string', () => {
    assert.strictEqual(cfg().get('server.path'), '');
  });

  test('customTags defaults to empty array', () => {
    assert.deepStrictEqual(cfg().get('customTags'), []);
  });

  test('keyOrdering defaults to false', () => {
    assert.strictEqual(cfg().get('keyOrdering'), false);
  });

  test('kubernetesVersion defaults to master', () => {
    assert.strictEqual(cfg().get('kubernetesVersion'), 'master');
  });

  test('schemaStore defaults to true', () => {
    assert.strictEqual(cfg().get('schemaStore'), true);
  });

  test('formatValidation defaults to true', () => {
    assert.strictEqual(cfg().get('formatValidation'), true);
  });

  test('formatPrintWidth defaults to 80', () => {
    assert.strictEqual(cfg().get('formatPrintWidth'), 80);
  });

  test('formatSingleQuote defaults to false', () => {
    assert.strictEqual(cfg().get('formatSingleQuote'), false);
  });

  test('httpProxy defaults to empty string', () => {
    assert.strictEqual(cfg().get('httpProxy'), '');
  });

  test('colorDecorators defaults to true', () => {
    assert.strictEqual(cfg().get('colorDecorators'), true);
  });

  test('schemas defaults to empty object', () => {
    assert.deepStrictEqual(cfg().get('schemas'), {});
  });
});

suite('configuration type correctness', () => {
  test('customTags value is an array', () => {
    assert.strictEqual(Array.isArray(cfg().get('customTags')), true);
  });

  test('schemas value is a plain object', () => {
    const val: unknown = cfg().get('schemas');
    assert.strictEqual(typeof val, 'object');
    assert.notStrictEqual(val, null);
    assert.strictEqual(Array.isArray(val), false);
  });
});
