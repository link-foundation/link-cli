// A client of a links server over TCP or any other inter-process
// communication transport: ports rust/src/protocol/client.rs (`LinksClient`).
//
// A server pushes the changes a connection subscribed to between its replies;
// a pushed document starts with `(notify: N)`. `request` sets the pushes it
// meets aside for `receiveNotification`, so replies and pushes never get
// mixed up. Every method that talks to the server is asynchronous.
import { connect as ipcConnect } from './ipc.js';
import { IoError, ProtocolError, protocolError } from './protocol-errors.js';
import { isNotification } from './protocol-links-operations.js';
import {
  errorMessage,
  formatDocument,
  isRef,
  parseDocument,
  queryDocument,
} from './protocol-lino.js';
import { LinoConnection } from './protocol-protocols.js';

const DIGITS = /^[0-9]+$/;

/** Sends substitution queries to a LiNo server over any protocol. */
export class LinksClient {
  #connection;
  #notifications = [];

  /** Talks over an already connected stream of any transport. */
  constructor(stream, protocol) {
    this.#connection = new LinoConnection(stream, protocol);
  }

  /** Talks over an already connected stream of any transport, `from_stream`. */
  static fromStream(stream, protocol) {
    return new LinksClient(stream, protocol);
  }

  /**
   * Connects to an IPC endpoint such as `127.0.0.1:8080`,
   * `unix:/tmp/links.sock` or `stdio`.
   */
  static async connectEndpoint(endpoint, protocol) {
    let stream;
    try {
      stream = await ipcConnect(endpoint);
    } catch (error) {
      throw protocolError(error);
    }
    return new LinksClient(stream, protocol);
  }

  /** Connects to the TCP `address`, `host:port`. */
  static connect(address, protocol) {
    return LinksClient.connectEndpoint(address, protocol);
  }

  /** The endpoint kind of the connection, such as `tcp` or `unix`. */
  kind() {
    return this.#connection.stream.kind;
  }

  /** The stream of the connection, to hang it up. */
  stream() {
    return this.#connection.stream;
  }

  /** Sends a parsed document and returns the reply; `(error: 'message')` becomes `Remote`. */
  async request(document) {
    await this.#connection.send(document);
    for (;;) {
      const reply = await this.#connection.receive();
      if (reply === null) throw ProtocolError.malformed('connection closed before the reply');
      if (isNotification(reply)) {
        this.#notifications.push(reply);
        continue;
      }
      const message = errorMessage(reply);
      if (message !== null) throw ProtocolError.remote(message);
      return reply;
    }
  }

  /** The number of pushes received and not yet taken by `receiveNotification`. */
  pendingNotifications() {
    return this.#notifications.length;
  }

  /**
   * The next document the server pushed, waiting up to `timeoutMs` for one;
   * null when none arrived in time. Fails when the connection closed, or when
   * the server sent something other than a push while no request waited.
   */
  async receiveNotification(timeoutMs) {
    if (this.#notifications.length > 0) return this.#notifications.shift();
    if (!(await this.#connection.waitForMessage(timeoutMs))) return null;
    const document = await this.#connection.receive();
    if (document === null) {
      throw ProtocolError.io(new IoError('UnexpectedEof', 'connection closed'));
    }
    if (isNotification(document)) return document;
    throw ProtocolError.malformed(
      `the server sent a reply nobody asked for: ${formatDocument(document)}`,
    );
  }

  /** Runs a LiNo substitution query; the empty query reads every link. */
  query(query) {
    return this.request(parseDocument(query));
  }

  /** Sends the original query text in a `(query: text)` envelope, keeping quoted references. */
  queryLossless(query) {
    return this.request(queryDocument(query));
  }

  /** Like `query`, returning the reply as canonical text. */
  async queryText(query) {
    return formatDocument(await this.query(query));
  }

  /**
   * Sends `(auth: token)`: every later request of the connection runs as the
   * subject the server gives the token, which this returns.
   */
  async authenticate(token) {
    const reply = await this.request([{ id: 'auth', values: [token] }]);
    if (reply.length === 1 && !isRef(reply[0]) && reply[0].id === 'subject') {
      const { values } = reply[0];
      if (values.length === 1 && isRef(values[0]) && DIGITS.test(values[0])) {
        const subject = Number(values[0]);
        if (subject <= 0xffffffff) return subject;
      }
    }
    throw ProtocolError.malformed(`expected a subject reply, found ${formatDocument(reply)}`);
  }
}
