// Run against the TCP entrance of an IPC stack backed by an empty database:
// node examples/ipc/javascript-client.mjs 127.0.0.1:7778
import assert from 'node:assert/strict';
import { connectTcp } from '../../js/client/src/tcp.js';

const client = await connectTcp(process.argv[2], { timeout: 5000 });
try {
  assert.equal(await client.count(), 0n);
  const link = await client.create(0n, 0n);
  assert.equal(await client.count(), 1n);
  assert.deepEqual(await client.each(), [link]);
  await client.delete(link.index);
  assert.equal(await client.count(), 0n);
  console.log('JavaScript client completed an IPC conversation');
} finally {
  await client.close();
}
