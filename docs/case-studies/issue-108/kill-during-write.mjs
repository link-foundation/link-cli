// Stops a clink server (SIGINT, as ServerProcess.close does) while writes are
// in flight, then reopens the database with a fresh server and counts the
// acknowledged writes that are gone.
//
//   node docs/case-studies/issue-108/kill-during-write.mjs /path/to/clink [rounds]
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { connectTcp } from '../../../js/client/src/tcp.js';
import { connectProcess, startServer } from '../../../js/client/src/process.js';

const command = process.argv[2];
const rounds = Number(process.argv[3] ?? 10);
let lostRounds = 0;
for (let round = 0; round < rounds; round += 1) {
  const dir = mkdtempSync(join(tmpdir(), 'kill during write '));
  const db = join(dir, 'db.links');
  try {
    const server = await startServer({ command, db });
    const client = await connectTcp(server.address);
    // Grow the file first, so a save takes long enough to be interrupted.
    await client.batch(Array.from({ length: 300 }, (_, i) => ({ op: 'create', source: BigInt(i + 1), target: BigInt(i + 1) })), { window: 32 });
    const controller = new AbortController();
    const pending = client.batch(
      Array.from({ length: 200 }, (_, i) => ({ op: 'create', source: BigInt(i + 1000), target: BigInt(i + 1000) })),
      { window: 8, signal: controller.signal },
    );
    setTimeout(() => {
      controller.abort();
      server.close();
    }, 20 + round);
    const result = await pending;
    await client.close();
    await server.exited;
    const acknowledged = 300 + result.ok;
    const reader = await connectProcess({ command, db });
    const stored = (await reader.each()).length;
    await reader.close();
    const lost = Math.max(0, acknowledged - stored);
    if (lost > 0) lostRounds += 1;
    console.log(`round ${round}: ${acknowledged} acknowledged, ${stored} stored after restart${lost ? `, ${lost} LOST` : ''}`);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}
console.log(`${lostRounds} of ${rounds} rounds lost acknowledged writes`);
