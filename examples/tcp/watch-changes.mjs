// Watches a links server for the changes another connection makes (issue
// #150): one connection subscribes to every link, a second one writes, and
// the first prints each `(notify: N)` push it receives.
//
// Usage (any clink; the Rust debug build by default):
//   cargo build --manifest-path rust/Cargo.toml --bin clink
//   node examples/tcp/watch-changes.mjs [path/to/clink]

import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { ANY } from '../../js/client/src/index.js';
import { startServer } from '../../js/client/src/process.js';
import { connectTcp } from '../../js/client/src/tcp.js';

const repo = fileURLToPath(new URL('../..', import.meta.url));
const clink = process.argv[2] ?? join(repo, 'rust/target/debug/clink');
const [command, args] = clink.endsWith('.dll') ? ['dotnet', [clink]] : [clink, []];
const dir = await mkdtemp(join(tmpdir(), 'watch-changes-'));
const server = await startServer({ command, args, db: join(dir, 'db.links') });
const show = (link) => (link ? `(${link.index}: ${link.source} ${link.target})` : '()');
try {
  const watcher = await connectTcp(server.address);
  const writer = await connectTcp(server.address);
  const all = await watcher.subscribe([ANY, ANY, ANY]);
  const selfLoops = await watcher.subscribe([ANY, 1, 1]);
  console.log(`watching every link as subscription ${all}, links (1 1) as ${selfLoops}`);
  watcher.onNotification(({ subscription, changes }) => {
    for (const { before, after } of changes) console.log(`(notify: ${subscription}) ${show(before)} -> ${show(after)}`);
  });

  await writer.create(0, 0);
  await writer.update(1, 1, 1);
  await writer.query('() ((2 2))');
  await writer.delete(1);
  await watcher.unsubscribe(all);
  await writer.create(3, 3); // nobody hears of this one
  // Replies come after the pushes of earlier writes: once this one is back,
  // every push above has been printed.
  console.log(`links left: ${await watcher.count()}`);
  await watcher.close();
  await writer.close();
} finally {
  await server.close();
  await rm(dir, { recursive: true, force: true });
}
