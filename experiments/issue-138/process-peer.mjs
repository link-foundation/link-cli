// Finite subprocess peer for process ownership/startup failure regressions.
import net from 'node:net';
import { writeFileSync } from 'node:fs';
import { MessageReader, encodeMessage } from '../../js/client/src/framing.js';

const [mode, pidPath] = process.argv.slice(2);
writeFileSync(pidPath, String(process.pid));
const lifetime = setTimeout(() => process.exit(9), 10000);
if (mode === 'exit') {
  process.stderr.write('injected startup failure\n');
  process.exit(3);
} else if (mode === 'invalid') {
  console.log('clink server listening on invalid-address');
} else if (mode === 'silent') {
  // Remain alive until the adapter cancels startup.
} else {
  const server = net.createServer((socket) => {
    socket.on('error', () => {});
    const reader = new MessageReader();
    socket.on('data', (bytes) => {
      for (const request of reader.push(bytes)) {
        socket.write(encodeMessage(mode === 'bad-peer' ? '(unsupported: ())' : '(count: 0)'));
        if (request !== '(count: (0))') process.stderr.write('request complete\n');
      }
    });
  });
  server.listen(0, '127.0.0.1', () => {
    // Split the banner to exercise stream framing, including unrelated output.
    process.stdout.write('startup diagnostics\nclink server lis');
    setImmediate(() => process.stdout.write(`tening on 127.0.0.1:${server.address().port}\n`));
  });
  if (mode === 'ignore-interrupt') process.on('SIGINT', () => {});
}
// An intentionally referenced timer bounds every failed test's child lifetime.
void lifetime;
