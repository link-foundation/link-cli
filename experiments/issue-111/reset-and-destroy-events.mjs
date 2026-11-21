// What the server end of a TCP connection sees when the client writes and then
// calls `resetAndDestroy()`, per Node.js version. Run with Node.js 20, 22, 26:
// node experiments/issue-111/reset-and-destroy-events.mjs [quiet|flushed]
// Node.js 20 and 22 close gracefully (FIN, `end`) when the write is still
// pending; once the write callback ran, every version resets (ECONNRESET).
import { connect, createServer } from 'node:net';

const server = createServer((socket) => {
  const events = [];
  socket.on('data', (chunk) => events.push(`data ${JSON.stringify(String(chunk))}`));
  socket.on('end', () => events.push('end'));
  socket.on('error', (error) => events.push(`error ${error.code}`));
  socket.on('close', (hadError) => {
    events.push(`close hadError=${hadError}`);
    console.log(process.version, events.join(', '));
    server.close();
  });
});
server.listen(0, '127.0.0.1', () => {
  const client = connect(server.address().port, '127.0.0.1', () => {
    const mode = process.argv[2];
    const text = mode === 'quiet' ? '' : 'GET /healthz HTTP/1.1\r\n';
    if (mode === 'flushed') client.write(text, () => client.resetAndDestroy());
    else {
      client.write(text);
      client.resetAndDestroy();
    }
  });
  client.on('error', (error) => console.log(process.version, 'client', error.code, error.syscall));
});
