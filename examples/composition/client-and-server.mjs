// A client's own links joined with a server's, the server's rules applied, and
// the whole read under UUIDs (issue #149). See README.md next to this file.
//
// Three clink processes play three stores:
//   server.links  the world of examples/permissions, served with
//                 --permissions self; this connection authenticates as alice
//   local.links   the client's own links
//   ids.links     the UUIDs given to the links of both, kept apart
//
// Usage (any clink; the Rust debug build by default):
//   cargo build --manifest-path rust/Cargo.toml --bin clink
//   node examples/composition/client-and-server.mjs [path/to/clink | path/to/clink.dll]

import { execFile } from 'node:child_process';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { ExternalIdMap, OffsetAddressMap, RemapLinks, UnionLinks } from '../../js/client/src/index.js';
import { connectProcess } from '../../js/client/src/process.js';

const WORLD =
  '() ((Type: Type Type) (Extends: Extends Extends) (AllowRead: AllowRead AllowRead) (AllowWrite: AllowWrite AllowWrite) (Anything: Anything Anything) ' +
  '(User: Type User) (Doc: Type Doc) (Public: Type Public) (Note: Doc Note) (Secret: Doc Secret) ' +
  '(alice: User alice) (note: Note note) (secret: Secret secret) (news: Public news) (AllowWrite (User Note)) (AllowRead (User Public)))';
// Where the server's links read: past every address of the client's store.
const SERVER = 1n << 24n;

const repo = fileURLToPath(new URL('../..', import.meta.url));
const clink = process.argv[2] ?? join(repo, 'rust/target/debug/clink');
const [command, args] = clink.endsWith('.dll') ? ['dotnet', [clink]] : [clink, []];
const run = (...more) => promisify(execFile)(command, [...args, ...more]);
const dir = await mkdtemp(join(tmpdir(), 'client-and-server-'));
const db = (name) => join(dir, `${name}.links`);

await run('--db', db('server'), '--auto-create-missing-references', WORLD);
await run('--db', db('server'), '--tokens', db('tokens'), '--subject', 'alice', '--grant-token', 'alice-secret');

const opened = [];
const open = async (name, more = []) => {
  const links = await connectProcess({ command, args: [...args, ...more], db: db(name) });
  opened.push(links);
  return links;
};
try {
  const server = await open('server', ['--permissions', 'self', '--tokens', db('tokens')]);
  console.log(`# the server's connection authenticates: ${(await server.requestText('(auth: alice-secret)')).trim()}`);
  const local = await open('local');
  const ids = await open('ids');

  // The union: the client's links first, the server's moved out of their way.
  const links = new UnionLinks([local, new RemapLinks(server, new OffsetAddressMap(SERVER))]);
  const show = async (view, link) => {
    const name = await view.getName(link.index);
    return `(${link.index}: ${link.source} ${link.target})${name === null ? '' : ` named ${name}`}`;
  };
  const print = async (view) => {
    for (const link of await view.each([])) console.log(`  ${await show(view, link)}`);
  };

  console.log('# what alice sees of the server, read through the union');
  await print(links);

  console.log("# a draft of the client that refers to the server's news");
  const news = await links.getByName('news');
  const draft = (await links.create(0n, 0n)).index;
  await links.update(draft, draft, news);
  await links.setName(draft, 'draft');
  console.log(`  created in member ${await links.ownerOf(draft)}: ${await show(links, (await links.each([draft]))[0])}`);

  console.log("# a write the server's rules refuse, through the union");
  try {
    // alice may read the news, not write it.
    await links.update(news, news, news);
  } catch (error) {
    console.log(`  ${error.message}`);
  }

  console.log('# the same links under UUIDs, kept in ids.links');
  let next = 0;
  const newId = () => `00000000-0000-4000-8000-${String((next += 1)).padStart(12, '0')}`;
  const byUuid = new RemapLinks(links, new ExternalIdMap(ids, 'uuid', { newId }));
  await print(byUuid);
  const uuid = await byUuid.toOuter(draft);
  console.log(`  the draft is ${uuid}; ids.links holds ${await ids.count([])} links, local.links ${await local.count([])}`);
} finally {
  for (const links of opened.reverse()) await links.close();
  await rm(dir, { recursive: true, force: true });
}
