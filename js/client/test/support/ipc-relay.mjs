// Relays standard input and output to an IPC endpoint through connectStream,
// byte for byte with half-close: `clink --connect exec:"node ipc-relay.mjs
// ENDPOINT"` then speaks either protocol through this client's transports.

import { connectStream } from '../../src/ipc.js';

const stream = await connectStream(process.argv[2]);
stream.on('error', (error) => {
  process.stderr.write(`ipc-relay: ${error.message}\n`);
  process.exitCode = 1;
});
process.stdin.pipe(stream);
stream.pipe(process.stdout);
