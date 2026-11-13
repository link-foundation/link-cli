// Run against any `clink --serve ENDPOINT` of an empty database, directly or
// through `--forward` relays:
//   node examples/ipc/javascript-client.mjs shm:/tmp/example-links
import assert from 'node:assert/strict';
import { connectIpc } from '../../js/client/src/ipc.js';

const client = await connectIpc(process.argv[2], { timeout: 5000 });
try {
  assert.equal(await client.count(), 0n);
  const link = await client.create(0n, 0n);
  assert.equal(await client.count(), 1n);
  assert.deepEqual(await client.each(), [link]);
  await client.delete(link.index);
  assert.equal(await client.count(), 0n);
  console.log(`JavaScript client completed a conversation over ${process.argv[2]}`);
} finally {
  await client.close();
}
