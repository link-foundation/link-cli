import assert from 'node:assert/strict';
import test from 'node:test';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { FileCommitLog } from '../src/engine-file-log.js';
import { encodeReferenceLiteral, decodeReferenceLiteral } from '../src/lino.js';
import { encodeMessage, MessageReader } from '../src/framing.js';

for (const text of ['\ufeff', '\ufeffleading', '\ufeff\ufeff']) {
  test('reference literal retains leading Unicode BOM: ' + JSON.stringify(text), () => {
    assert.equal(decodeReferenceLiteral(encodeReferenceLiteral(text)), text);
  });
  test('message reader retains leading Unicode BOM: ' + JSON.stringify(text), () => {
    const bytes = encodeMessage(text); const reader = new MessageReader();
    assert.deepEqual(reader.push(bytes), [text]);
    const fragmented = new MessageReader();
    assert.deepEqual([...bytes].flatMap(byte => fragmented.push(Uint8Array.of(byte))), [text]);
  });
  test('file journal retains leading Unicode BOM: ' + JSON.stringify(text), () => {
    const directory = mkdtempSync(join(tmpdir(), 'clink-utf8-'));
    try { using log = new FileCommitLog(join(directory, 'journal')); log.append(text); log.flush(); assert.deepEqual(JSON.parse(log.read()), [text]); }
    finally { rmSync(directory, { recursive: true, force: true }); }
  });
}
