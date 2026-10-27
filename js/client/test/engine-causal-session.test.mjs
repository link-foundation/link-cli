import test from 'node:test';
import assert from 'node:assert/strict';
import { CausalSession } from '../src/engine-causal-session.js';
import { CausalVersion } from '../src/engine-causal-version.js';

const token = n => BigInt(n).toString(16).padStart(32, '0');
const author = token(1);
const version = CausalVersion.advance('1', author, '0', [], { source: '0', target: '2', name: 'old' });
const edit = () => ({ address: '1', observed: [CausalVersion.encode(version)], payload: { source: '2', target: '0', name: 'new' } });
function fixture() {
  const state = { requests: [], freed: 0, finished: 0, calls: 0, inputs: [], edits: [edit()] };
  class Workspace {
    constructor(...inputs) { state.inputs.push(inputs); }
    request(text) { const operation = JSON.parse(text); state.requests.push(operation); if (state.requestError) throw new Error('request'); return JSON.stringify(operation[0] === 'rows' ? state.rows ?? [] : state.reply ?? null); }
    finish() { state.finished++; if (state.finishError) throw new Error('finish'); return JSON.stringify(state.edits); }
    free() { state.freed++; }
  }
  const batch = {
    async snapshot() { return { generation: '7', versions: [version] }; },
    async write(...request) {
      state.calls++; state.publication = structuredClone(request);
      if (state.failure) { state.failure = false; throw new Error('indeterminate'); }
      await state.gate;
      return [CausalVersion.advance('1', author, '1', [version], { source: '2', target: '0', name: 'new' })];
    },
  };
  return { state, binding: { CausalLinksWorkspace: Workspace }, batch };
}
const open = f => CausalSession.open(f.binding, f.batch, '32', '33', 'max-dot');

test('preparation reserves synchronously, drains queued edits and always frees the native handle', async () => {
  const f = fixture(), session = await open(f);
  const operation = session.setName('1', 'new');
  const preparing = session.prepare(token(100));
  await assert.rejects(session.deleteLink('1'), /belongs|closed/i);
  await operation;
  const request = await preparing;
  assert.deepEqual(f.state.requests, [['set-name', '1', 'new'], ['rows']]);
  assert.equal(request.generation, '7'); assert.equal(request.token, token(100));
  assert.deepEqual(request.edits[0].observed, [version]);
  assert.equal(f.state.finished, 1); assert.equal(f.state.freed, 1); assert.equal(f.state.calls, 0);
  request.edits[0].observed.length = 0; request.edits[0].payload.name = 'caller mutation';
  assert.equal((await session.prepare(token(100))).edits[0].payload.name, 'new');
  await assert.rejects(session.prepare(token(200)));
  await session.close(); await session.close(); assert.equal(f.state.freed, 1);
});

test('unknown publication freezes the original request and retries exactly once per attempt', async () => {
  const f = fixture(), session = await open(f); f.state.failure = true;
  await assert.rejects(session.commit(token(100)), /indeterminate/);
  const original = structuredClone(f.state.publication);
  await assert.rejects(session.createLink('0', '0'));
  const rejectedToken = assert.rejects(session.commit(token(200)));
  const [left, right] = await Promise.all([session.commit(token(100)), session.commit(token(100))]);
  await rejectedToken;
  assert.equal(f.state.calls, 2); assert.deepEqual(f.state.publication, original);
  left[0].payload.name = 'caller mutation'; assert.equal(right[0].payload.name, 'new');
  assert.equal((await session.commit(token(100)))[0].payload.name, 'new'); assert.equal(f.state.calls, 2);
  assert.equal(session.prepared.edits[0].observed.length, 1);
  const borrowed = session.prepared; borrowed.edits.length = 0; assert.equal(session.prepared.edits.length, 1);
  await session.close(); await assert.rejects(session.commit(token(100)));
});

test('a queued request failure leaves the transfer tail usable and preparation failure releases ownership', async () => {
  const f = fixture(), session = await open(f); f.state.requestError = true;
  await assert.rejects(session.getLink('1'), /request/); f.state.finishError = true;
  await assert.rejects(session.prepare(token(100)), /finish/);
  assert.equal(f.state.freed, 1); assert.equal(f.state.calls, 0); assert.equal(session.prepared, null);
  await session.close(); assert.equal(f.state.freed, 1);
});

test('malformed tokens reject before reservation and invalid final edits reject before real publication', async () => {
  for (const invalid of ['', 'invalid', '0'.repeat(32), 'F'.repeat(32)]) {
    const f = fixture(), session = await open(f);
    await assert.rejects(session.prepare(invalid));
    await session.getLink('1'); assert.equal(f.state.finished, 0); assert.equal(f.state.calls, 0);
    const rejected = assert.rejects(session.commit(invalid));
    const valid = session.commit(token(100));
    await rejected; await valid; assert.equal(f.state.calls, 1);
    await session.close(); assert.equal(f.state.freed, 1);
  }
  for (const invalid of [[{ ...edit(), payload: { source: '0', target: '0', name: 'a'.repeat(4097) } }], Array.from({ length: 65 }, edit)]) {
    const f = fixture(), session = await open(f); f.state.edits = invalid;
    await assert.rejects(session.commit(token(100))); assert.equal(f.state.calls, 0); assert.equal(f.state.freed, 1);
    await session.close();
  }
  const f = fixture(), session = await open(f); f.state.edits.push({ ...edit(), address: '2' });
  await assert.rejects(session.commit('f'.repeat(32))); assert.equal(f.state.calls, 0); assert.equal(f.state.freed, 1);
});

test('all raw methods preserve canonical u64 strings and queued replies', async () => {
  const f = fixture(), session = await open(f); f.state.reply = '18446744073709551615';
  assert.equal(await session.createLink('0', '18446744073709551615'), f.state.reply);
  await session.ensureCreated('32'); await session.getLink('1'); await session.updateLink('1', '0', '2');
  await session.deleteLink('1'); await session.getName('1'); await session.getByName('雪');
  await session.removeName('1'); await session.rows();
  assert.deepEqual(f.state.requests.map(v => v[0]), ['create', 'ensure', 'get', 'update', 'delete', 'get-name', 'get-by-name', 'set-name', 'rows']);
  assert.deepEqual(f.state.requests[7], ['set-name', '1', null]);
  await session.rollback(); await session.rollback(); assert.equal(f.state.freed, 1);
  await assert.rejects(session.rows()); await assert.rejects(session.prepare(token(100)));
});

test('empty receipts cache owned results and close waits for an in-flight real publication', async () => {
  const f = fixture(), session = await open(f); f.state.edits = [];
  let release; f.state.gate = new Promise(resolve => { release = resolve; });
  const commit = session.commit(token(100));
  while (f.state.calls === 0) await Promise.resolve();
  let closed = false; const closing = session.close().then(() => { closed = true; });
  await Promise.resolve(); assert.equal(closed, false); release(); await commit; await closing;
  assert.equal(f.state.calls, 1); assert.equal(f.state.freed, 1);
});

test('opening owns options before awaiting the snapshot and validates binding and batch capabilities', async () => {
  const f = fixture(); let resolve;
  f.batch.snapshot = () => new Promise(done => { resolve = done; });
  const options = { stateStructure: 'shared', constraints: ['unique-names'] };
  const pending = CausalSession.open(f.binding, f.batch, '32', '33', 'reject', options);
  options.constraints.length = 0; resolve({ generation: '7', versions: [version] });
  const session = await pending;
  assert.deepEqual(JSON.parse(f.state.inputs[0][4]), { stateStructure: 'shared', constraints: ['unique-names'] });
  await session.close();
  await assert.rejects(CausalSession.open({}, f.batch, '32', '33', 'reject'));
  await assert.rejects(CausalSession.open(f.binding, {}, '32', '33', 'reject'));
});

test('frozen reads own their rows and preserve absent and empty names', async () => {
  const f = fixture(), session = await open(f);
  f.state.rows = [{ index: '1', source: '2', target: '0', name: '' }];
  await session.prepare(token(100));
  assert.deepEqual(await session.getLink('1'), { index: '1', source: '2', target: '0' });
  assert.equal(await session.getLink('9'), null);
  assert.equal(await session.getName('1'), ''); assert.equal(await session.getName('9'), null);
  assert.equal(await session.getByName(''), '1'); assert.equal(await session.getByName('absent'), null);
  const rows = await session.rows(); rows[0].name = 'caller mutation';
  assert.equal((await session.rows())[0].name, '');
  await session.commit(token(100));
  assert.equal((await Promise.all([session.commit(token(100)), session.commit(token(100))]))[1][0].payload.name, 'new');
  assert.equal(f.state.calls, 1); await session.close();
});

test('close reaps a rejected accepted commit and malformed bindings release acquired ownership', async () => {
  const f = fixture(), session = await open(f); f.state.failure = true;
  const commit = session.commit(token(100)); const close = session.close();
  await assert.rejects(commit); await close; assert.equal(f.state.freed, 1);
  let freed = 0;
  await assert.rejects(CausalSession.open({ CausalLinksWorkspace: class { free() { freed++; } } }, fixture().batch, '1', '2', 'reject'));
  assert.equal(freed, 1);
  await assert.rejects(CausalSession.open({ CausalLinksWorkspace: class {} }, fixture().batch, '1', '2', 'reject'));
});

test('ordinary client preserves original query text, full u64 operations and queued ownership', async () => {
  const f = fixture(), session = await open(f), client = session.client();
  assert.equal(client.capabilities.maxAddress, 18446744073709551615n);
  assert.equal(client.capabilities.losslessQueries, true);
  assert.equal(client.capabilities.atomicTransactions, true);
  client.require('atomicTransactions', 'rollback');
  f.state.reply = '() ((18446744073709551615: 0 0))';
  assert.deepEqual(await client.create(0n, 0n), { index: 18446744073709551615n, source: 0n, target: 0n });
  const text = "(('4294967312': $s $t)) (('4294967312': $t $s))";
  f.state.reply = '';
  assert.deepEqual(await client.query(text), []);
  const { parseDocument } = await import('../src/lino.js');
  assert.deepEqual(parseDocument(f.state.requests[1][1]), [{ id: 'query', values: [text] }]);
  assert.equal(f.state.requests[0][0], 'lino');
  const abort = new AbortController();
  const pending = client.count([], { signal: abort.signal }); abort.abort();
  await assert.rejects(pending, error => error.outcome === 'not-sent');
  assert.equal(f.state.requests.length, 2);
  const request = await session.prepare(token(100)); assert.equal(request.edits.length, 1);
  await assert.rejects(client.create(0n, 0n), error => error.outcome === 'not-sent');
  await client.close(); assert.equal(f.state.freed, 1);
  await assert.rejects(client.count(), error => error.outcome === 'not-sent');
});

test('lossless causal batches preflight malformed and bounded queries before any write', async () => {
  const f = fixture(), session = await open(f), client = session.client();
  for (const text of ['(', '('.repeat(70) + 'x' + ')'.repeat(70), 'x '.repeat(4097), 'x'.repeat(1048577)]) {
    await assert.rejects(client.batch([{ op: 'create', source: 0n, target: 0n }, { op: 'query', text }]));
    assert.equal(f.state.requests.length, 0);
  }
  f.state.reply = '';
  const text = "(('18446744073709551615': $s $t)) ()";
  assert.equal((await client.batch([{ op: 'query', text }])).completed, true);
  const { parseDocument } = await import('../src/lino.js');
  assert.deepEqual(parseDocument(f.state.requests[0][1]), [{ id: 'query', values: [text] }]);
  await session.close();
});
