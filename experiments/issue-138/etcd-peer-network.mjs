// Link-level network partitions for a real etcd cluster, without root.
//
// Jepsen cuts links with iptables. Here every member advertises a peer URL
// served by a proxy in this process, which forwards to the member's real peer
// listener. etcd's rafthttp names the sending member of every peer connection
// in the X-Server-From header of its first request, so the proxy knows which
// link a connection belongs to and in which direction its Raft messages flow:
//
// - `POST /raft...` (pipeline, snapshot) and probes are dialed by the sender
//   and carry messages from the dialer to the member behind the proxy;
// - `GET /raft/stream/...` is dialed by the receiver, and the member behind
//   the proxy streams messages back to the dialer.
//
// Cutting the directed link a -> b destroys every open connection that
// carries messages from a to b, and holds new ones open without forwarding a
// byte (a black hole: the dialer sees a timeout, not a refusal). Messages from
// b to a still flow, so a cut can be one-way, which iptables cannot do for TCP.
// Healing destroys the held connections; etcd redials at once.
import net from 'node:net';

const HEADER_LIMIT = 16384;

export class PeerNetwork {
  #proxies = [];
  #connections = new Set();
  #cut = new Set();
  #ids = new Map();
  #trace = () => {};
  #senders = new Set();
  /**
   * `targets[i]` is member i's real peer port; resolves once all proxies
   * listen. `trace`, when given, receives a line per classified connection.
   */
  static async start(targets, { trace } = {}) {
    const network = new PeerNetwork();
    if (trace) network.#trace = trace;
    for (const [index, target] of targets.entries())
      network.#proxies.push(await network.#listen(index, target));
    return network;
  }
  /** Proxy ports, in member order, for `--initial-advertise-peer-urls`. */
  get ports() {
    return this.#proxies.map((server) => server.address().port);
  }
  /** Map etcd member ids (decimal strings, as the v3 JSON gateway returns them) to indices. */
  learn(members) {
    for (const [index, id] of members.entries()) this.#ids.set(BigInt(id).toString(16), index);
  }
  /** Cut each directed link `[from, to]`, replacing the previous cuts. */
  partition(links) {
    this.#cut = new Set(links.map(([from, to]) => `${from}>${to}`));
    this.#trace(`cut ${this.cuts.join(' ') || 'nothing'}`);
    for (const connection of this.#connections)
      if (this.#isCut(this.#link(connection))) connection.destroy();
  }
  /** Cut every link between `group` and the others, both ways. */
  isolate(group, size) {
    const inside = new Set(group);
    const links = [];
    for (let a = 0; a < size; a++)
      for (let b = 0; b < size; b++) if (inside.has(a) !== inside.has(b)) links.push([a, b]);
    this.partition(links);
  }
  heal() {
    this.#cut = new Set();
    this.#trace(`heal`);
    for (const connection of this.#connections) if (connection.held) connection.destroy();
  }
  get cuts() {
    return [...this.#cut];
  }
  /** Senders seen in X-Server-From that `learn` was not told about. */
  get strangers() {
    return [...this.#senders].filter((from) => !this.#ids.has(from));
  }
  async close() {
    this.#cut = new Set();
    for (const connection of this.#connections) connection.destroy();
    await Promise.all(this.#proxies.map((server) => new Promise((done) => server.close(done))));
  }
  #isCut(link) {
    return link !== null && this.#cut.has(`${link[0]}>${link[1]}`);
  }
  async #listen(index, target) {
    const server = net.createServer((socket) => this.#accept(index, target, socket));
    await new Promise((done, reject) => {
      server.once('error', reject);
      server.listen(0, '127.0.0.1', done);
    });
    return server;
  }
  #accept(index, target, socket) {
    const connection = {
      index,
      from: undefined,
      stream: false,
      held: false,
      sockets: [socket],
    };
    connection.destroy = () => {
      for (const s of connection.sockets) s.destroy();
      this.#connections.delete(connection);
    };
    this.#connections.add(connection);
    socket.on('error', () => {});
    socket.on('close', () => connection.destroy());
    let head = Buffer.alloc(0);
    const onData = (chunk) => {
      head = Buffer.concat([head, chunk]);
      const end = head.indexOf('\r\n\r\n');
      if (end < 0 && head.length < HEADER_LIMIT) return;
      socket.off('data', onData);
      socket.pause();
      const text = head.subarray(0, end < 0 ? head.length : end).toString('latin1');
      connection.from = /^x-server-from:\s*([0-9a-f]+)\s*$/im.exec(text)?.[1];
      connection.stream = /^GET \/raft\/stream\//.test(text);
      const link = this.#link(connection);
      this.#trace(
        `proxy n${index}: ${text.slice(0, text.indexOf('\r\n'))} from ${connection.from} carries ${link ? `n${link[0]}>n${link[1]}` : 'no messages'}${this.#isCut(link) ? ', cut' : ''}`,
      );
      if (this.#isCut(link)) {
        // Black hole: read and discard, never reach the member.
        connection.held = true;
        socket.on('data', () => {});
        socket.resume();
        return;
      }
      const upstream = net.connect(target, '127.0.0.1');
      connection.sockets.push(upstream);
      upstream.on('error', () => {});
      upstream.on('close', () => connection.destroy());
      upstream.pipe(socket);
      // etcd's prober shares the pipeline's HTTP transport, so a keep-alive
      // connection opened by an unlabeled `GET /raft/probing` later carries
      // `POST /raft` messages. Until a request names its sender, scan what
      // the dialer sends; a request's headers cannot complete upstream
      // before the line naming the sender has been seen here.
      let tail = '';
      const forward = (chunk) => {
        if (connection.from === undefined) {
          const text = tail + chunk.toString('latin1');
          const sender = /x-server-from:[ \t]*([0-9a-f]+)\r\n/i.exec(text)?.[1];
          tail = text.slice(-64);
          if (sender !== undefined) {
            connection.from = sender;
            const later = this.#link(connection);
            this.#trace(
              `proxy n${index}: a later request from ${sender} carries ${later ? `n${later[0]}>n${later[1]}` : 'no messages'}${this.#isCut(later) ? ', cut' : ''}`,
            );
            if (this.#isCut(later)) {
              connection.destroy();
              return;
            }
          }
        }
        if (!upstream.write(chunk)) {
          socket.pause();
          upstream.once('drain', () => socket.resume());
        }
      };
      forward(head);
      socket.on('data', forward);
      socket.resume();
    };
    socket.on('data', onData);
  }
  // The directed link whose Raft messages a connection carries, or null for
  // traffic that names no known sender (version probes, member lists).
  #link({ index, from, stream }) {
    if (from === undefined) return null;
    this.#senders.add(from);
    const sender = this.#ids.get(from);
    if (sender === undefined) return null;
    return stream ? [index, sender] : [sender, index];
  }
}
