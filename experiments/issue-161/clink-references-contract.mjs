// Replays the requests of packages/ui-links/tests/references.test.js (link-foundation/ui)
// against a real clink server, with the world and the launcher of that repository.
//
//   CLINK=/path/to/clink node experiments/issue-161/clink-references-contract.mjs /path/to/ui
//
// `#` in an expected reply stands for any address. Exits non-zero on a difference.
import { pathToFileURL } from 'node:url';
import { join, resolve } from 'node:path';

const repo = resolve(process.argv[2] ?? '');
const load = (path) => import(pathToFileURL(join(repo, path)).href);
const { world } = await load('packages/ui-links/tests/helpers.js');
const { startClink } = await load('examples/lib/ui-links-clink.mjs');
const { allow } = await load('packages/ui-links/src/index.js');
const { connectLinks } = await load('packages/ui-links/src/tcp.js');

const MISSING = 9999;
let failed = 0;

function check(name, reply, expected) {
  const pattern = expected.replace(/[.*+?^${}()|[\]\\]/g, '\\$&').replaceAll('#', '\\d+');
  const ok = new RegExp(`^${pattern}$`).test(reply.trim());
  if (!ok) failed += 1;
  console.log(`${ok ? 'ok  ' : 'DIFF'} ${name}`);
  if (!ok) console.log(`       expected ${expected}\n       actual   ${reply.trim()}`);
  return reply;
}

async function scenario(name, prepare, body) {
  console.log(`# ${name}`);
  const data = world();
  prepare?.(data);
  const tokens = new Map([
    ['alice-token', data.v.alice],
    ['boss-token', data.v.boss],
  ]);
  const clink = await startClink({ ...data, tokens });
  const opened = [];
  const session = async (token) => {
    const transport = await connectLinks(clink.address);
    opened.push(transport);
    await transport.send(`(auth: ${token})`);
    return (request) => transport.send(request);
  };
  try {
    await body({ ...data, alice: await session('alice-token'), boss: await session('boss-token') });
  } finally {
    for (const transport of opened) await transport.close();
    await clink.close();
  }
}

const created = (reply) => Number(/\((\d+):/.exec(reply)?.[1]);

await scenario('references without the right', null, async ({ alice, t, v }) => {
  const own = created(await alice(`(create: (${t.Note} 0))`));
  const requests = {
    '(create: (Note address))': (a) => `(create: (${t.Note} ${a}))`,
    '(create: (0 address))': (a) => `(create: (0 ${a}))`,
    '(update: (own Note address))': (a) => `(update: (${own} ${t.Note} ${a}))`,
  };
  for (const [name, request] of Object.entries(requests)) {
    check(
      `${name}, unreadable`,
      await alice(request(v.secret)),
      `(error: 'Link not found: ${v.secret}')`,
    );
    check(
      `${name}, missing`,
      await alice(request(MISSING)),
      `(error: 'Link not found: ${MISSING}')`,
    );
  }
  for (const [name, request] of Object.entries({
    'a query': (a) => `() ((${t.Note} ${a}))`,
    'a query that picks the address': (a) => `() ((${a}: ${t.Note} ${t.Note}))`,
  })) {
    const hidden = await alice(request(v.secret));
    const missing = await alice(request(MISSING));
    check(
      `${name}, alike`,
      hidden.replaceAll(String(v.secret), 'N'),
      missing.replaceAll(String(MISSING), 'N').trim(),
    );
    console.log(`       answer: ${hidden.trim()}`);
  }
  for (const type of [t.Secret, MISSING]) {
    check(
      `a type it may not write (${type === MISSING ? 'missing' : 'unreadable'})`,
      await alice(`(update: (${own} ${type} ${own}))`),
      `(error: 'Forbidden: update ${own}')`,
    );
  }
});

await scenario('names', null, async ({ alice, boss, t, v }) => {
  // The operations of clink's protocol take numbers: a name is no address there, known or not.
  for (const request of [
    (n) => `(create: (${t.Note} ${n}))`,
    (n) => `(each: (${n}))`,
    (n) => `() ((${t.Note} ${n}))`,
  ]) {
    const hidden = await alice(request('secret'));
    const unknown = await alice(request('nothing'));
    check(
      `${request('NAME')}, alike`,
      hidden.replaceAll('secret', 'N'),
      unknown.replaceAll('nothing', 'N').trim(),
    );
    console.log(`       answer: ${hidden.trim()}`);
  }
  check(
    '(set-name:) takes a free name',
    await alice(`(set-name: (${v.note} 'fresh'))`),
    `(link: #)`,
  );
  check(
    '(set-name:) takes a hidden name',
    await alice(`(set-name: (${v.note} 'secret'))`),
    `(error: 'Forbidden: set-name ${v.note}')`,
  );
  check(
    'a query takes a hidden name',
    await alice(`() ((secret: ${t.Note} ${t.Note}))`),
    `(error: 'Forbidden: set-name #')`,
  );
  check('the name stays', await boss("(get-by-name: 'secret')"), `(link: ${v.secret})`);
});

await scenario(
  'an AllowReference rule on a type',
  ({ links, markers, t }) => allow(links, markers, 'reference', t.User, t.Secret),
  async ({ alice, t, v }) => {
    check(
      'a link of the type',
      await alice(`(create: (${t.Note} ${v.secret}))`),
      `() ((#: ${t.Note} ${v.secret}))`,
    );
    check(
      'the type itself',
      await alice(`(create: (${t.Note} ${t.Secret}))`),
      `() ((#: ${t.Note} ${t.Secret}))`,
    );
    check(
      'not its sibling',
      await alice(`(create: (${t.Note} ${t.Admin}))`),
      `(error: 'Link not found: ${t.Admin}')`,
    );
    check(
      'missing stays missing',
      await alice(`(create: (${t.Note} ${MISSING}))`),
      `(error: 'Link not found: ${MISSING}')`,
    );
    check('does not read: (each:)', await alice(`(each: (${v.secret}))`), '');
    check('does not read: (get-by-name:)', await alice("(get-by-name: 'secret')"), '');
    check(
      'does not delete',
      await alice(`(delete: ${v.secret})`),
      `(error: 'Link not found: ${v.secret}')`,
    );
  },
);

await scenario('a rule that names the type', null, async ({ alice, t, v }) => {
  check('Public is not readable', await alice(`(each: (${t.Public}))`), '');
  check('Note', await alice(`(create: (${t.Note} ${t.Note}))`), `() ((#: ${t.Note} ${t.Note}))`);
  check(
    'Public',
    await alice(`(create: (${t.Note} ${t.Public}))`),
    `() ((#: ${t.Note} ${t.Public}))`,
  );
  check(
    'a readable link',
    await alice(`(create: (${t.Note} ${v.shared}))`),
    `() ((#: ${t.Note} ${v.shared}))`,
  );
  check(
    'the subject itself',
    await alice(`(create: (${t.Note} ${v.alice}))`),
    `() ((#: ${t.Note} ${v.alice}))`,
  );
});

await scenario('a reference the subject did not add', null, async ({ alice, boss, t, v }) => {
  const n = created(await boss(`(create: (${t.Note} ${v.secret}))`));
  check(
    'may stay',
    await alice(`(update: (${n} ${t.Shared} ${v.secret}))`),
    `((${n}: ${t.Note} ${v.secret})) ((${n}: ${t.Shared} ${v.secret}))`,
  );
  await alice(`(update: (${n} ${t.Shared} ${n}))`);
  check(
    'cannot be put back',
    await alice(`(update: (${n} ${t.Shared} ${v.secret}))`),
    `(error: 'Link not found: ${v.secret}')`,
  );
});

// A store is unique by (source target), so (Anything Anything) is the point Anything itself.
// The world of link-foundation/ui keeps a second doublet for it, which --import merges into
// the point without moving the later mark, so the mark on the point is written here over the protocol.
await scenario('Anything to Anything', null, async ({ alice, boss, markers, t, v }) => {
  check(
    'before the rule',
    await alice(`(create: (${t.Note} ${v.secret}))`),
    `(error: 'Link not found: ${v.secret}')`,
  );
  check(
    'the mark',
    await boss(`(create: (${markers.allowReference} ${markers.anything}))`),
    `() ((#: ${markers.allowReference} ${markers.anything}))`,
  );
  check(
    'an unreadable link',
    await alice(`(create: (${t.Note} ${v.secret}))`),
    `() ((#: ${t.Note} ${v.secret}))`,
  );
  // Known difference from ui-links, which refuses it: as before #161, nothing checks a target
  // that a subject may refer to, so it is stored; hidden and missing still tell nothing apart.
  check(
    'a missing one',
    await alice(`(create: (${t.Note} ${MISSING}))`),
    `() ((#: ${t.Note} ${MISSING}))`,
  );
  check(
    'its address is taken',
    await alice(`() ((${v.secret}: ${t.Note} ${t.Note}))`),
    `(error: 'Link ${v.secret} exists')`,
  );
  check('still not readable', await alice(`(each: (${v.secret}))`), '');
});

await scenario('a subject that may read everything', null, async ({ boss, t, v }) => {
  check(
    'an unreadable link',
    await boss(`(create: (${t.Note} ${v.secret}))`),
    `() ((#: ${t.Note} ${v.secret}))`,
  );
});

console.log(failed === 0 ? 'all alike' : `${failed} difference(s)`);
process.exit(failed === 0 ? 0 : 1);
