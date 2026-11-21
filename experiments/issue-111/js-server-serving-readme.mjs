// Checks the serving snippet of js/server/README.md: a protocol server on a
// free TCP port, one client query, then shutdown.
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  LinksClient,
  LinksServer,
  NamedTypesDecorator,
  TextLinoProtocol,
} from '../../js/server/src/index.js';

const directory = mkdtempSync(join(tmpdir(), 'clink-serve-'));
try {
  // --- README snippet ---
  const store = NamedTypesDecorator.open(join(directory, 'db.links'));
  const server = await LinksServer.listen('tcp:127.0.0.1:0');
  const stop = server.shutdownHandle();
  const serving = server.serve(store);

  const client = await LinksClient.connect(server.endpoint(), new TextLinoProtocol());
  console.log(await client.queryText('() ((1 1))')); // () ((1: 1 1))
  client.stream().shutdown();

  stop.shutdown();
  await serving;
  store.save();
  // --- end ---
} finally {
  rmSync(directory, { recursive: true, force: true });
}
