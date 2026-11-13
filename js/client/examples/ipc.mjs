// node js/client/examples/ipc.mjs ENDPOINT
//
// Connects to `clink --serve ENDPOINT` over any IPC endpoint, e.g.
//   clink --db db.links --serve unix:/tmp/links.sock
//   node js/client/examples/ipc.mjs unix:/tmp/links.sock
// Set CLINK_IPC_TRACE=1 to see the connect and handshake on standard error.
import { connectIpc, parseEndpoint, supports, supportsNatively } from '../src/ipc.js';

const text = process.argv[2];
if (!text) {
  console.error('usage: node examples/ipc.mjs ENDPOINT (e.g. unix:/tmp/links.sock)');
  process.exit(2);
}
const endpoint = parseEndpoint(text);
const how = supportsNatively(endpoint.kind)
  ? 'natively'
  : supports(endpoint.kind)
    ? 'through the clink bridge'
    : 'not at all on this platform';
console.log(`${endpoint} (${endpoint.kind}): connecting ${how}`);

const links = await connectIpc(endpoint, { timeout: 10_000 });
try {
  console.log('transport', links.capabilities.transport);
  const link = await links.create(1n, 1n);
  console.log('created', link);
  console.log('links in the store', await links.count());
  const changes = await links.delete(link.index);
  console.log('deleted', changes);
} finally {
  await links.close();
}
