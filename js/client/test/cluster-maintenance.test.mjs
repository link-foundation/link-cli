import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { EtcdJournalStore } from '../src/cluster.js';
const vectors = JSON.parse(readFileSync(new URL('../../../docs/protocol/journal-maintenance-v1.json', import.meta.url)));
const b64 = (text) => btoa(unescape(encodeURIComponent(text)));
const response = (value) => new Response(JSON.stringify(value));
function fixture(replies, prefix = '/test/') {
  const calls = [];
  const store = new EtcdJournalStore(['http://one:1'], { prefix, fetch: async (_url, request) => {
    calls.push(JSON.parse(request.body)); assert.ok(calls.length <= replies.length); return response(replies[calls.length - 1]);
  } });
  return { store, calls };
}
for (const vector of vectors.scans) test(`maintenance scan: ${vector.name}`, { timeout: 5000 }, async () => {
  const { store, calls } = fixture(vector.replies, vector.prefix);
  if (vector.expected === null) await assert.rejects(store.scanNamespace(vector.options));
  else {
    assert.deepEqual(await store.scanNamespace(vector.options), vector.expected);
    assert.equal(calls[0].success[0].request_range.revision, undefined);
    if (calls.length > 1) {
      assert.equal(calls[1].success[0].request_range.revision, '10');
      assert.equal(calls[1].success[0].request_range.key, b64(vector.prefix + 'b\0'));
    }
  }
  assert.equal(calls.length, vector.replies.length);
});
for (const vector of vectors.deletes) test(`maintenance delete: ${vector.name}`, { timeout: 5000 }, async () => {
  const { store, calls } = fixture([vector.reply]);
  if (vector.expected === null) await assert.rejects(store.compareDelete('a', '7'), { outcome: 'unknown' });
  else assert.equal(await store.compareDelete('a', '7'), vector.expected);
  assert.equal(calls.length, 1);
  assert.deepEqual(calls[0], { compare: [{ key: b64('/test/a'), target: 'MOD', result: 'EQUAL', mod_revision: '7' }], success: [{ request_delete_range: { key: b64('/test/a') } }] });
});
test('maintenance options and delete identity fail before I/O', { timeout: 5000 }, async () => {
  const { store, calls } = fixture([]);
  for (const options of [{ maxKeys: 0 }, { maxBytes: 0 }, { pageKeys: 0 }, { pageKeys: 257 }, { keysOnly: 1 }, { maxKeys: 1.5 }])
    await assert.rejects(store.scanNamespace(options), TypeError);
  for (const generation of ['0', '01', '9223372036854775808', '7 ', 7]) await assert.rejects(store.compareDelete('a', generation), TypeError);
  for (const key of ['', '/', 7]) await assert.rejects(store.compareDelete(key, '7'), TypeError);
  assert.deepEqual(calls, []);
});
test('scan failure restarts the entire snapshot on the next endpoint', { timeout: 5000 }, async () => {
  const page = vectors.scans.find((item) => item.name === 'pinned pagination').replies[0]; const calls = [];
  const store = new EtcdJournalStore(['http://one:1', 'http://two:2'], { prefix: '/test/', fetch: async (url, request) => {
    const body = JSON.parse(request.body); calls.push({ url, body });
    if (calls.length === 1) return response(page);
    if (calls.length === 2) throw Error('revision compacted or connection lost');
    return response({ header: { revision: '12' }, succeeded: true, responses: [{ response_range: {} }] });
  } });
  assert.deepEqual(await store.scanNamespace({ pageKeys: 2 }), []);
  assert.equal(calls.length, 3); assert.match(calls[2].url, /two/);
  assert.equal(calls[2].body.success[0].request_range.revision, undefined);
  assert.equal(calls[2].body.success[0].request_range.key, b64('/test/'));
});
test('lost delete responses are never retried on another endpoint', { timeout: 5000 }, async () => {
  let calls = 0;
  const store = new EtcdJournalStore(['http://one:1', 'http://two:2'], { fetch: async () => { calls++; throw Error('lost after commit'); } });
  await assert.rejects(store.compareDelete('a', '7'), { outcome: 'unknown' }); assert.equal(calls, 1);
});
test('default scan options send bounded nonserializable ranges', { timeout: 5000 }, async () => {
  const { store, calls } = fixture([vectors.scans[0].replies[0]]);
  assert.deepEqual(await store.scanNamespace(), []);
  assert.deepEqual(calls[0].success[0].request_range, { key: b64('/test/'), range_end: b64('/test0'), limit: '128', sort_order: 'ASCEND', sort_target: 'KEY', keys_only: false });
});
