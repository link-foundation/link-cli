import test from 'node:test';
import assert from 'node:assert/strict';
import { WasmEngine } from '../src/index.js';
test('compactJournal preserves synchronous native boolean and rejects closed wrapper', () => {
  class Native {
    result = true;
    compactJournal() { const result = this.result; this.result = false; return result; }
    free() {}
  }
  const engine = new WasmEngine({ StagedEngine: Native });
  assert.equal(engine.compactJournal(), true);
  assert.equal(engine.compactJournal(), false);
  engine.close(); assert.throws(() => engine.compactJournal(), /closed/);
});
