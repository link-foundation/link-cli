// `clink --serve ADDR --graphql`: Hasura's GraphQL over HTTP and WebSocket
// for Deep, over the store. Port of `run_graphql_server` in rust/src/main.rs.
//
// SIGINT, SIGTERM and SIGHUP stop the server (the `termination` feature of
// the `ctrlc` crate); the store is saved when it stops.

import { readFileSync } from 'node:fs';

import { Settings } from './graphql-execute.js';
import { Json, utf8Text } from './graphql-json.js';
import { GraphqlServer } from './graphql-server.js';
import { DeepStore } from './graphql-store.js';
import { Endpoint } from './ipc-endpoint.js';

const SIGNALS = ['SIGINT', 'SIGTERM', 'SIGHUP'];

/** `std::fs::read_to_string`: the text of a file, which must be UTF-8. */
function readText(file) {
  const bytes = readFileSync(file);
  try {
    return utf8Text(bytes);
  } catch {
    return null;
  }
}

/** The rows of a Deep package, or the message of the JSON that does not parse. */
function parsePackage(text) {
  try {
    return { rows: Json.parse(text) };
  } catch (error) {
    return { problem: error.message };
  }
}

/**
 * Serves `storage` over GraphQL on the TCP endpoint `address` until a signal
 * stops it; `cli` gives `serveAlso`, `deepPackages`, `jwtSecret` and
 * `graphqlAdminSecret`. `io.stdout` takes the banner, `io.signals` (the
 * process) the stop signals; `io.lookup` stands for `dns.lookup`.
 */
export async function runGraphqlServer(cli, address, storage, policy, io = {}) {
  if (cli.serveAlso.length > 0) {
    throw new Error('--graphql serves HTTP on one TCP address; give --serve once.');
  }
  const endpoint = Endpoint.parse(address);
  if (endpoint.kind !== 'tcp') {
    throw new Error(`--graphql serves HTTP over TCP only, not over \`${endpoint.kind}:\`.`);
  }
  const store = DeepStore.open(storage, policy);
  for (const file of cli.deepPackages) {
    const text = readText(file);
    if (text === null) throw new Error('stream did not contain valid UTF-8');
    const { rows, problem } = parsePackage(text);
    if (problem !== undefined) throw new Error(`${file}: ${problem}`);
    store.loadPackage(rows);
  }
  const secret =
    cli.jwtSecret == null ? Settings.randomSecret() : Buffer.from(cli.jwtSecret, 'utf8');
  const settings = new Settings(secret, cli.graphqlAdminSecret ?? null);
  const server = await GraphqlServer.bind(endpoint.address, { lookup: io.lookup });
  const signals = io.signals ?? process;
  const stop = () => server.shutdown();
  for (const signal of SIGNALS) signals.on(signal, stop);
  (io.stdout ?? process.stdout).write(`clink GraphQL server listening on ${server.localAddr()}\n`);
  await server.serve(store, settings);
  for (const signal of SIGNALS) signals.off(signal, stop);
  storage.save();
}
