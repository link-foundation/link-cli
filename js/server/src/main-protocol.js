// The protocol commands of `clink`: ports `binary_options`, `run_client`,
// `run_server`, `run_relay` and `print_banner` of rust/src/main.rs, with the
// `ctrlc` handler (its `termination` feature: SIGINT and SIGTERM) that stops
// a server or a relay.
//
// `io` is `{ stdout, stderr, signals }`: the streams default to the process
// ones and `signals` (anything with `on` and `off`) to `process`.
import { ArityRange } from './protocol-binary-packet.js';
import { BinaryLinoOptions } from './protocol-binary.js';
import { LinkError } from './errors.js';
import { Endpoint, asciiLower, trimRust as trim } from './ipc-endpoint.js';
import { listenAll, relay } from './ipc.js';
import { LinksClient } from './protocol-client.js';
import { BinaryLinoProtocol, TextLinoProtocol } from './protocol-protocols.js';
import { AcceptedProtocols, LinksServer, ServerOptions } from './protocol-server.js';
import { LinksSessions } from './protocol-sessions.js';
import { LinksTokenTable } from './permissions-tokens.js';

const STOP_SIGNALS = ['SIGINT', 'SIGTERM'];

/** `eq_ignore_ascii_case`. */
const named = (text, name) => asciiLower(text) === name;

/** The binary options of `--external-references`, `--arity` and `--packed-widths`. */
export function binaryOptions(cli) {
  return new BinaryLinoOptions({
    externalReferences: cli.externalReferences,
    arity: new ArityRange(cli.arity.min, cli.arity.max),
    packedWidths: cli.packedWidths,
  });
}

/** Calls `stop` on SIGINT or SIGTERM until the returned function is called. */
function onStopSignals(signals, stop) {
  const handler = () => stop();
  for (const signal of STOP_SIGNALS) signals.on(signal, handler);
  return () => {
    for (const signal of STOP_SIGNALS) signals.off(signal, handler);
  };
}

/** `--connect`: sends the query to a server and prints the reply; a binary option implies binary. */
export async function runClient(cli, address, io = {}) {
  const out = io.stdout ?? process.stdout;
  const options = binaryOptions(cli);
  const plain = options.equals(new BinaryLinoOptions());
  let protocol;
  const name = cli.protocol == null ? null : trim(cli.protocol);
  if (name === null) {
    protocol = plain ? new TextLinoProtocol() : new BinaryLinoProtocol({ options });
  } else if (named(name, 'text')) {
    if (!plain) throw new LinkError('Binary protocol options require --protocol binary.');
    protocol = new TextLinoProtocol();
  } else if (named(name, 'binary')) {
    protocol = new BinaryLinoProtocol({ options });
  } else {
    throw new LinkError(`Invalid --protocol value '${name}'. Use 'text' or 'binary'.`);
  }
  const query = cli.query ?? cli.queryArg;
  const client = await LinksClient.connectEndpoint(address, protocol);
  try {
    if (cli.token != null) await client.authenticate(cli.token);
    const reply = await client.queryText(query ?? '');
    if (reply !== '') out.write(`${reply}\n`);
  } finally {
    client.stream().shutdown();
  }
}

/** The `--protocol` of `--serve`. */
function acceptedProtocols(protocol) {
  if (protocol == null) return AcceptedProtocols.ANY;
  const name = trim(protocol);
  for (const accept of [AcceptedProtocols.ANY, AcceptedProtocols.TEXT, AcceptedProtocols.BINARY]) {
    if (named(name, accept)) return accept;
  }
  throw new LinkError(`Invalid --protocol value '${name}'. Use 'any', 'text' or 'binary'.`);
}

/**
 * `--serve`: serves `storage` until SIGINT or SIGTERM. With `policy` every
 * connection sees the store as its subject does: `subject` until
 * `(auth: token)` finds another in the token table: none without `tokens`,
 * the data of `storage` for `{ store: null }`, `tokens.store` otherwise.
 */
export async function runServer(cli, storage, policy, subject, tokens, io = {}) {
  if (cli.query != null || cli.queryArg != null) {
    throw new LinkError('--serve does not take a query; send queries with --connect.');
  }
  const options = new ServerOptions({
    trace: cli.trace,
    autoCreateMissingReferences: cli.autoCreateMissingReferences,
    accept: acceptedProtocols(cli.protocol),
    triplets: cli.triplets,
  });
  let sessions = (
    policy == null ? new LinksSessions(storage) : LinksSessions.withPermissions(storage, policy)
  )
    .withSubject(subject)
    .withAutoCreateMissingReferences(cli.autoCreateMissingReferences);
  if (tokens != null) {
    sessions = sessions.withAuthenticator((store, token) =>
      new LinksTokenTable(tokens.store ?? store).authenticate(token),
    );
  }
  const server = await LinksServer.listenAll(cli.serveEndpoints(), options);
  const stop = server.shutdownHandle();
  const release = onStopSignals(io.signals ?? process, () => stop.shutdown());
  try {
    printBanner(server.endpoints(), io);
    await server.serveSessions(sessions);
    storage.save();
  } finally {
    release();
    // Rust leaves open connections to the end of the process.
    server.disconnectAll();
  }
}

/**
 * `--serve A --forward B`: relays every connection accepted on the `--serve`
 * endpoints, byte for byte, to a new connection to `B`; no store is opened.
 */
export async function runRelay(cli, target, io = {}) {
  if (cli.serve.length === 0) {
    throw new LinkError('--forward relays the connections of --serve; give --serve as well.');
  }
  if (cli.query != null || cli.queryArg != null) {
    throw new LinkError('--forward does not take a query; send queries with --connect.');
  }
  if (cli.graphql) throw new LinkError('--forward and --graphql cannot be used together.');
  const endpoint = Endpoint.parse(target);
  const listener = await listenAll(cli.serveEndpoints());
  const release = onStopSignals(io.signals ?? process, () => listener.close());
  try {
    printBanner(listener.endpoints(), io);
    if (cli.trace) (io.stderr ?? process.stderr).write(`clink: forwarding to ${endpoint}\n`);
    await relay(listener, endpoint);
  } finally {
    release();
    listener.close();
  }
}

/** Whether `endpoint` is a `stdio` endpoint; text that does not parse is not. */
function isStdio(endpoint) {
  try {
    return Endpoint.parse(endpoint).kind === 'stdio';
  } catch {
    return false;
  }
}

/**
 * Prints `clink server listening on ENDPOINT` once per endpoint: on standard
 * output, or on standard error when one endpoint is `stdio`, whose standard
 * output carries the protocol.
 */
export function printBanner(endpoints, io = {}) {
  const stream = endpoints.some(isStdio)
    ? (io.stderr ?? process.stderr)
    : (io.stdout ?? process.stdout);
  for (const endpoint of endpoints) stream.write(`clink server listening on ${endpoint}\n`);
}
