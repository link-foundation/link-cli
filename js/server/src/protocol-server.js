// A server exposing a links store through the LiNo protocols over TCP or any
// other inter-process communication transport: ports
// rust/src/protocol/server.rs (`LinksServer`, `ServerOptions`,
// `AcceptedProtocols`, `ShutdownHandle`, `accept_loop`, `handle_connection`
// and `serve_connection`). The dispatcher, connections and outboxes are in
// protocol-server-dispatch.js.
//
// Every connection reads its messages and writes its outbox in tasks of its
// own; every request is executed by `serveSessions`, one at a time, over the
// synchronous store. The protocol is detected per message and the reply uses
// the same protocol, so text and binary clients can share one server.
import {
  IoError,
  ProtocolError,
  alternateText,
  diagnostics,
  protocolError,
} from './protocol-errors.js';
import { errorText } from './error-report.js';
import { listen as ipcListen, listenAll as ipcListenAll } from './ipc.js';
import { errorDocument, formatDocument } from './protocol-lino.js';
import {
  MessageFormat,
  ProtocolLimits,
  StreamReader,
  readAnyDocumentWithQueryLimits,
} from './protocol-protocols.js';
import { LinksSessions } from './protocol-sessions.js';
import { executeRequestFor } from './protocol-server-query.js';
import {
  DEFAULT_MAX_PENDING_NOTIFICATIONS,
  Dispatcher,
  JobQueue,
  Outbox,
  SHUTDOWN,
  admitted,
  closeJob,
  requestJob,
} from './protocol-server-dispatch.js';
import { QueryProcessor } from './query-processor.js';

export { DEFAULT_MAX_PENDING_NOTIFICATIONS };

/** Which protocols a server accepts: `any` (the default), `text` or `binary`. */
export const AcceptedProtocols = Object.freeze({
  ANY: 'any',
  TEXT: 'text',
  BINARY: 'binary',
  /** Whether `accept` answers a message in `format`. */
  accepts(accept, format) {
    if (accept === 'text') return !format.isBinary();
    if (accept === 'binary') return format.isBinary();
    return true;
  },
});

/** Server configuration, `ServerOptions` and its `Default`. */
export class ServerOptions {
  constructor({
    trace = false,
    autoCreateMissingReferences = false,
    accept = AcceptedProtocols.ANY,
    limits = new ProtocolLimits(),
    maxPendingNotifications = DEFAULT_MAX_PENDING_NOTIFICATIONS,
    triplets = false,
  } = {}) {
    this.trace = trace;
    this.autoCreateMissingReferences = autoCreateMissingReferences;
    this.accept = accept;
    this.limits = limits;
    this.maxPendingNotifications = maxPendingNotifications;
    this.triplets = triplets;
  }
}

/** Stops a running `serveSessions`. */
export class ShutdownHandle {
  #state;

  constructor(state) {
    this.#state = state;
  }

  /** Asks the server to stop; `serveSessions` returns after the current request. */
  shutdown() {
    this.#state.stopping = true;
    this.#state.jobs.send(SHUTDOWN);
    // Wake the accept loop so it notices the flag.
    this.#state.listener.close();
  }
}

// Every connection of the process gets its own number.
let connections = 0;

/** A bound, not yet serving, LiNo server. */
export class LinksServer {
  #state;
  #readable = null;

  constructor(listener, options = new ServerOptions()) {
    this.#state = {
      listener,
      options,
      jobs: new JobQueue(),
      stopping: false,
      outboxes: new Set(),
    };
  }

  /** Listens on an IPC endpoint such as `127.0.0.1:0`, `unix:/tmp/links.sock` or `stdio`. */
  static async listen(endpoint, options = new ServerOptions()) {
    return new LinksServer(await wrapped(() => ipcListen(endpoint)), options);
  }

  /** Listens on every endpoint at once; their clients share the store. */
  static async listenAll(endpoints, options = new ServerOptions()) {
    return new LinksServer(await wrapped(() => ipcListenAll(endpoints)), options);
  }

  /** Serves the connections of any listener, such as one of a registered transport. */
  static fromListener(listener, options = new ServerOptions()) {
    return new LinksServer(listener, options);
  }

  /**
   * Lets subscribers see only the links `readable` accepts. In triplet mode
   * it judges the outer doublet of a triplet, `(index: type index)`.
   */
  withReadable(readable) {
    this.#readable = readable;
    return this;
  }

  /** The TCP address the server listens on; an error for one on no TCP endpoint. */
  localAddr() {
    const address = this.#state.listener.localAddr();
    if (address == null) {
      throw ProtocolError.io(
        new IoError('Unsupported', `\`${this.endpoint()}\` is not a TCP endpoint`),
      );
    }
    return address;
  }

  /** The endpoints the server listens on, joined by spaces. */
  endpoint() {
    return this.#state.listener.endpoint();
  }

  /** Every endpoint the server listens on, in the order given. */
  endpoints() {
    return this.#state.listener.endpoints();
  }

  /** A handle that stops `serveSessions`. */
  shutdownHandle() {
    return new ShutdownHandle(this.#state);
  }

  /** Serves `storage` until stopped; every connection sees and changes the whole store. */
  serve(storage) {
    const auto = this.#state.options.autoCreateMissingReferences;
    return this.serveSessions(new LinksSessions(storage).withAutoCreateMissingReferences(auto));
  }

  /** Serves `storage` without sessions or subscriptions, `serve_for`. */
  async serveFor(storage) {
    const state = this.#state;
    const { options } = state;
    const acceptor = acceptLoop(state, storage.queryLimits());
    const processor = new QueryProcessor(false).withAutoCreateMissingReferences(
      options.autoCreateMissingReferences,
    );
    try {
      for (;;) {
        const job = await state.jobs.recv();
        if (job.kind === 'shutdown') break;
        if (job.kind === 'close') continue;
        if (options.trace && admitted(storage, job.document)) {
          diagnostics.stderr(`[server] request: ${formatDocument(job.document)}`);
        }
        const response = executeRequestFor(storage, processor, job.document);
        if (options.trace) diagnostics.stderr(`[server] reply: ${formatDocument(response)}`);
        job.outbox.reply(response, job.format);
        job.done(true);
      }
    } finally {
      stop(state);
    }
    await acceptor;
  }

  /**
   * Serves `sessions` until a `ShutdownHandle` stops the server: every
   * connection is a session of its own, opened with the default subject of
   * `sessions` and closed when the connection closes.
   */
  async serveSessions(sessions) {
    const state = this.#state;
    const { options } = state;
    if (options.triplets) {
      try {
        sessions.ensureTriplets();
      } catch (error) {
        state.listener.close();
        throw ProtocolError.io(new IoError('Other', alternateText(error)));
      }
    }
    const acceptor = acceptLoop(state, sessions.store().queryLimits());
    const dispatcher = new Dispatcher(options, this.#readable);
    try {
      for (;;) {
        const job = await state.jobs.recv();
        if (!dispatcher.run(sessions, job)) break;
      }
      dispatcher.closeAll(sessions);
    } finally {
      stop(state);
    }
    await acceptor;
  }

  /**
   * Hangs up every connection still open. Rust leaves them to the end of the
   * process; a JavaScript runtime waits for them instead, so the CLI calls
   * this once the store is saved.
   */
  disconnectAll() {
    for (const outbox of [...this.#state.outboxes]) outbox.close();
  }
}

/**
 * Once serving stops, no request may touch the store, and the listener
 * accepts no more (Rust drops it with the accept thread).
 */
function stop(state) {
  state.stopping = true;
  state.jobs.close();
  state.listener.close();
}

async function wrapped(operation) {
  try {
    return await operation();
  } catch (error) {
    throw protocolError(error);
  }
}

/** The pause after a failed accept, so a failing listener does not spin. */
export const ACCEPT_RETRY_MS = 10;

/**
 * Accepts until the listener is closed or can accept no more (a `stdio`
 * listener after its only connection), then stops the server.
 */
async function acceptLoop(state, queryLimits) {
  const { listener, options, jobs } = state;
  for (;;) {
    if (state.stopping) return;
    let stream;
    try {
      stream = await listener.accept();
    } catch (error) {
      if (options.trace) diagnostics.stderr(`[server] accept failed: ${errorText(error)}`);
      await new Promise((resolve) => setTimeout(resolve, ACCEPT_RETRY_MS));
      continue;
    }
    if (stream === null) {
      if (options.trace && !state.stopping) {
        diagnostics.stderr(`[server] ${listener.endpoint()} accepts no more connections`);
      }
      jobs.send(SHUTDOWN);
      return;
    }
    if (state.stopping) {
      stream.shutdown();
      return;
    }
    if (options.trace) diagnostics.stderr(`[server] ${stream.kind} connection accepted`);
    handleConnection(state, stream, queryLimits).catch((error) => {
      if (options.trace) diagnostics.stderr(`[server] connection closed: ${errorText(error)}`);
    });
  }
}

async function handleConnection(state, stream, queryLimits) {
  const { options, jobs } = state;
  const connection = connections;
  connections += 1;
  const outbox = new Outbox(options.maxPendingNotifications, () => stream.shutdown());
  state.outboxes.add(outbox);
  const writer = outbox.writeAll(stream, options.limits).catch((error) => {
    if (options.trace) diagnostics.stderr(`[server] connection closed: ${errorText(error)}`);
  });
  try {
    return await serveConnection(state, stream, connection, outbox, queryLimits);
  } finally {
    // Its subscriptions go with it.
    jobs.send(closeJob(connection));
    outbox.close();
    await writer;
    state.outboxes.delete(outbox);
  }
}

async function serveConnection(state, stream, connection, outbox, queryLimits) {
  const { options, jobs } = state;
  const reader = new StreamReader(stream);
  for (;;) {
    let message;
    try {
      message = await readAnyDocumentWithQueryLimits(reader, options.limits, queryLimits);
    } catch (error) {
      if (outbox.isClosed()) return;
      // The stream may be out of sync; answer in text and hang up.
      outbox.reply(errorDocument(errorText(error)), MessageFormat.TEXT);
      await outbox.waitForReplies();
      throw error;
    }
    if (message === null) {
      if (options.trace) diagnostics.stderr('[server] client hung up');
      return;
    }
    const [document, format] = message;
    if (AcceptedProtocols.accepts(options.accept, format)) {
      const ran = await new Promise((resolve) => {
        if (!jobs.send(requestJob(connection, outbox, document, format, resolve))) resolve(false);
      });
      if (!ran) outbox.reply(errorDocument('server is shutting down'), format);
    } else {
      outbox.reply(errorDocument('this server does not accept this protocol'), format);
    }
    await outbox.waitForReplies();
    if (outbox.isClosed()) {
      if (options.trace) diagnostics.stderr('[server] connection closed');
      return;
    }
  }
}
