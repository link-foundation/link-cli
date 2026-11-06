// Acceptance scenario: real DeepClient against a GraphQL endpoint given as host:port/path.
const { generateApolloClient } = require('@deep-foundation/hasura/client.js');
const assert = require('assert');
(async () => {
  const { DeepClient } = await import('@deep-foundation/deeplinks/imports/client.js');
  const path = process.argv[2];
  const make = (token) => generateApolloClient({ path, ssl: false, ws: true, token });
  const anon = new DeepClient({ apolloClient: make() });
  const g = await anon.guest();
  console.log('guest', g.linkId, !!g.token);
  assert(g.linkId && g.token);
  const deep = new DeepClient({ apolloClient: make(g.token), linkId: g.linkId, token: g.token });
  const w = await deep.jwt({}).catch((e) => ({ error: String(e) }));
  console.log('whoami', w.linkId, w.error);
  const some = await deep.select({ id: { _lte: 5 } }, { order_by: { id: 'asc' } });
  console.log(
    'select',
    some.data.map((l) => [l.id, l.type_id, l.from_id, l.to_id]),
  );
  const typeId = 1;
  const seen = [];
  const sub = deep
    .subscribe({
      type_id: typeId,
      from_id: 0,
      to_id: 0,
      id: { _gt: 0 },
      order_by: { id: 'desc' },
      limit: 1,
    })
    .subscribe({
      next: (v) => {
        seen.push(v);
        console.log('sub', JSON.stringify(v).slice(0, 100));
      },
      error: (e) => console.log('suberr', e),
    });
  await new Promise((r) => setTimeout(r, 1500));
  const ins = await deep.insert({ type_id: typeId, from_id: 0, to_id: 0 }, { name: 'INS' });
  console.log('insert', ins.data);
  const id = ins.data[0].id;
  await new Promise((r) => setTimeout(r, 2500));
  sub.unsubscribe();
  console.log('updates', seen.length);
  const ins2 = await deep
    .insert({ type_id: typeId, string: { data: { value: 'hello' } } }, { name: 'INSV' })
    .catch((e) => ({ err: String(e) }));
  console.log('insertv', ins2.data || ins2.err);
  if (ins2.data) {
    const r = await deep.select(
      { id: ins2.data[0].id },
      { returning: 'id value string { value }' },
    );
    console.log('value', JSON.stringify(r.data));
    const u = await deep
      .update(
        { link_id: ins2.data[0].id },
        { value: 'bye' },
        { table: 'strings', returning: 'id value' },
      )
      .catch((e) => ({ err: String(e) }));
    console.log('update', JSON.stringify(u.data || u.err));
    await deep.delete(ins2.data[0].id);
  }
  const rel = await deep
    .select(
      {
        type_id: 3,
        from: { type_id: 2, string: { value: '@deep-foundation/core' } },
        string: { value: 'Type' },
      },
      { returning: 'id to_id' },
    )
    .catch((e) => ({ err: String(e) }));
  console.log('rel', JSON.stringify(rel.data || rel.err));
  try {
    console.log('deep.id', await deep.id('@deep-foundation/core', 'Package'));
  } catch (e) {
    console.log('iderr', String(e));
  }
  const cnt = await deep
    .select({ id: { _lte: 5 } }, { aggregate: 'count' })
    .catch((e) => ({ err: String(e) }));
  console.log('count', JSON.stringify(cnt.data ?? cnt.err));
  const can = await deep.can(id, g.linkId, 1).catch((e) => String(e));
  console.log('can', can);
  const ser = await deep
    .serial({ operations: [{ type: 'insert', table: 'links', objects: { type_id: typeId } }] })
    .catch((e) => ({ err: String(e) }));
  console.log('serial', JSON.stringify(ser.data || ser.err));
  const del = await deep.delete(id);
  console.log('delete', del.data);
  const after = await deep.select({ id });
  assert.equal(after.data.length, 0);
  console.log('E2E OK');
  process.exit(0);
})().catch((e) => {
  console.error('E2E FAIL', e);
  process.exit(1);
});
