// Cross-language IPC interop (issue #140): every endpoint kind of this
// platform carries the text and the binary LiNo protocol between every pair of
// clink implementations, as server, client and `--forward` relay.
//
//   node examples/ipc/interop.mjs rust=rust/target/release/clink \
//     'csharp=["dotnet","csharp/Foundation.Data.Doublets.Cli/bin/Release/net10.0/clink.dll"]'
//
// Each NAME=COMMAND names an implementation; COMMAND is a JSON array of
// strings or words separated by spaces. `--kinds tcp,shm` limits the kinds;
// `--js` adds the JS client of js/client (its bridge kinds need CLINK_BIN).
// It exits with 1 when any conversation fails, after trying all of them.
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

const PLATFORM_KINDS = {
  linux: [
    'tcp',
    'udp',
    'unix',
    'pipe',
    'fifo',
    'stdio',
    'shm',
    'mmap',
    'file',
    'signal',
    'mq',
    'sysvmq',
  ],
  darwin: [
    'tcp',
    'udp',
    'unix',
    'pipe',
    'fifo',
    'stdio',
    'shm',
    'mmap',
    'file',
    'signal',
    'sysvmq',
  ],
  win32: [
    'tcp',
    'udp',
    'pipe',
    'stdio',
    'shm',
    'mmap',
    'file',
    'signal',
    'mailslot',
    'copydata',
    'alpc',
  ],
};
const BANNER = /clink server listening on (.*?)\r?\n/;
const TIMEOUT = 60_000;

const words = (text) => (text.trim().startsWith('[') ? JSON.parse(text) : text.trim().split(/\s+/));

function parseArguments(argv) {
  const implementations = [];
  let kinds = PLATFORM_KINDS[process.platform] ?? PLATFORM_KINDS.linux;
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === '--kinds') {
      kinds = argv[++i].split(',');
      continue;
    }
    if (argv[i] === '--js') {
      implementations.push({ name: 'js', js: true });
      continue;
    }
    const equals = argv[i].indexOf('=');
    if (equals <= 0) throw new Error(`expected NAME=COMMAND, got \`${argv[i]}\``);
    implementations.push({
      name: argv[i].slice(0, equals),
      command: words(argv[i].slice(equals + 1)),
    });
  }
  if (!implementations.some((implementation) => !implementation.js)) {
    throw new Error('name at least one NAME=COMMAND implementation');
  }
  return { implementations, kinds };
}

/** Runs one clink command to its end: `{ code, stdout, stderr }`. */
function run(implementation, args) {
  const [program, ...rest] = implementation.command;
  return new Promise((resolve) => {
    const child = spawn(program, [...rest, ...args], { stdio: ['ignore', 'pipe', 'pipe'] });
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (chunk) => (stdout += chunk));
    child.stderr.on('data', (chunk) => (stderr += chunk));
    const timer = setTimeout(() => {
      stderr += `\n(killed after ${TIMEOUT} ms)`;
      child.kill('SIGKILL');
    }, TIMEOUT);
    child.once('error', (error) => {
      clearTimeout(timer);
      resolve({ code: -1, stdout, stderr: `${stderr}${error.message}` });
    });
    child.once('close', (code) => {
      clearTimeout(timer);
      resolve({ code, stdout: stdout.replace(/\r\n/g, '\n'), stderr });
    });
  });
}

/** Starts a long-running `clink --serve ...` and resolves once its banner names the endpoint. */
function start(implementation, args) {
  const [program, ...rest] = implementation.command;
  const child = spawn(program, [...rest, ...args], { stdio: ['ignore', 'pipe', 'pipe'] });
  let stderr = '';
  child.stderr.on('data', (chunk) => (stderr = (stderr + chunk).slice(-4096)));
  const exited = new Promise((resolve) => child.once('close', resolve));
  const listening = new Promise((resolve, reject) => {
    let stdout = '';
    const timer = setTimeout(
      () => reject(new Error(`no banner in ${TIMEOUT} ms: ${stderr}`)),
      TIMEOUT,
    );
    child.stdout.on('data', (chunk) => {
      stdout += chunk;
      const match = BANNER.exec(stdout);
      if (match) {
        clearTimeout(timer);
        resolve(match[1]);
      }
    });
    child.once('error', reject);
    child.once('close', (code) => {
      clearTimeout(timer);
      reject(new Error(`exited with ${code} before listening: ${stderr}`));
    });
  });
  return {
    listening,
    async stop() {
      if (child.exitCode === null && child.signalCode === null) child.kill();
      await exited;
    },
  };
}

let sequence = 0;

/** A fresh `--serve` endpoint of `kind` inside `dir`. */
function endpointOf(kind, dir) {
  const name = `n${++sequence}`;
  switch (kind) {
    case 'tcp':
      return '127.0.0.1:0';
    case 'udp':
      return 'udp:127.0.0.1:0';
    case 'unix':
      return `unix:${path.join(dir, `${name}.sock`)}`;
    case 'pipe':
      return process.platform === 'win32'
        ? `pipe:link-cli-interop-${process.pid}-${name}`
        : `pipe:${path.join(dir, name)}`;
    default:
      return `${kind}:${path.join(dir, name)}`;
  }
}

const results = [];

async function check(label, conversation) {
  const began = Date.now();
  try {
    await conversation();
    results.push({ label, ok: true });
    console.log(`ok    ${label} (${Date.now() - began} ms)`);
  } catch (error) {
    results.push({ label, ok: false });
    console.log(`FAIL  ${label}: ${error.message}`);
  }
}

const ipc = await import(new URL('../../js/client/src/ipc.js', import.meta.url).href);

/** A JS client conversation: creates the point link `(n n)` through `endpoint` and reads it back. */
async function createWithJs(endpoint, n) {
  const client = await ipc.connectIpc(endpoint, { timeout: 30_000 });
  try {
    const before = await client.count();
    const link = await client.create(BigInt(n), BigInt(n));
    if ((await client.count()) !== before + 1n || link.source !== BigInt(n)) {
      throw new Error(`the JS client created ${JSON.stringify(link, (_, v) => String(v))}`);
    }
  } finally {
    await client.close();
  }
}

/** Sends `() ((n n))` from `client` over `endpoint` in `protocol` and checks the reply. */
async function create(client, endpoint, protocol, n) {
  if (client.js) return createWithJs(endpoint, n);
  const reply = await run(client, [
    '--connect',
    endpoint,
    '--protocol',
    protocol,
    `() ((${n} ${n}))`,
  ]);
  if (reply.code !== 0 || !new RegExp(`\\(\\d+: ${n} ${n}\\)`).test(reply.stdout)) {
    throw new Error(`${protocol} create answered ${reply.code}: ${reply.stdout}${reply.stderr}`);
  }
}

/** Reads every link through `endpoint` and checks that each of `numbers` is there. */
async function expectAll(client, endpoint, numbers) {
  const reply = await run(client, ['--connect', endpoint, '']);
  const missing = numbers.filter((n) => !reply.stdout.includes(`: ${n} ${n})`));
  if (reply.code !== 0 || missing.length > 0) {
    throw new Error(
      `read answered ${reply.code}, missing ${missing.join(' ')}: ${reply.stdout}${reply.stderr}`,
    );
  }
}

/** The protocols `client` speaks: the JS client speaks text. */
const protocolsOf = (client) => (client.js ? ['text'] : ['text', 'binary']);

/** Whether `client` can reach `kind` here. */
const reaches = (client, kind) => !client.js || ipc.supports(kind === 'stdio' ? 'exec' : kind);

/** Every client speaks both protocols to `server` over `kind`. */
async function serveAndConnect(kind, server, implementations, dir) {
  const clients = implementations.filter((client) => reaches(client, kind));
  const reader = clients.find((client) => !client.js);
  if (kind === 'stdio') {
    for (const client of clients) {
      const db = path.join(dir, `stdio-${server.name}-${client.name}.links`);
      const endpoint = `exec:${JSON.stringify([...server.command, '--db', db, '--serve', 'stdio'])}`;
      await check(`stdio: ${server.name} server (exec) <- ${client.name} client`, async () => {
        const protocols = protocolsOf(client);
        for (const [i, protocol] of protocols.entries())
          await create(client, endpoint, protocol, i + 1);
        await expectAll(
          reader,
          endpoint,
          protocols.map((_, i) => i + 1),
        );
      });
    }
    return;
  }
  const db = path.join(dir, `${kind}-${server.name}.links`);
  const running = start(server, ['--db', db, '--serve', endpointOf(kind, dir)]);
  let endpoint;
  await check(`${kind}: ${server.name} server listens`, async () => {
    endpoint = await running.listening;
  });
  try {
    if (endpoint === undefined) return;
    let n = 0;
    const numbers = [];
    for (const client of clients) {
      for (const protocol of protocolsOf(client)) {
        const value = ++n;
        await check(
          `${kind}: ${server.name} server <- ${client.name} client, ${protocol}`,
          async () => {
            await create(client, endpoint, protocol, value);
            numbers.push(value);
          },
        );
      }
    }
    await check(`${kind}: ${server.name} server keeps every write`, () =>
      expectAll(reader, endpoint, numbers),
    );
  } finally {
    await running.stop();
  }
}

/** `relay` serves `kind` and forwards to `server` over TCP; every client talks through the stack. */
async function relayAndConnect(kind, relay, server, implementations, dir) {
  const clients = implementations.filter((client) => reaches(client, kind));
  const db = path.join(dir, `relay-${kind}-${relay.name}-${server.name}.links`);
  const backend = start(server, ['--db', db, '--serve', '127.0.0.1:0']);
  let relaying;
  try {
    const target = await backend.listening;
    relaying = start(relay, ['--serve', endpointOf(kind, dir), '--forward', target]);
    const endpoint = await relaying.listening;
    let n = 0;
    for (const client of clients) {
      const value = ++n;
      await check(
        `${kind}: ${client.name} client -> ${relay.name} relay -> ${server.name} server (tcp)`,
        () => create(client, endpoint, value % 2 === 0 ? 'binary' : 'text', value),
      );
    }
  } catch (error) {
    await check(`${kind}: ${relay.name} relay -> ${server.name} server`, () =>
      Promise.reject(error),
    );
  } finally {
    await relaying?.stop();
    await backend.stop();
  }
}

const { implementations, kinds } = parseArguments(process.argv.slice(2));
const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'clink-interop-'));
try {
  for (const kind of kinds) {
    for (const server of implementations.filter((implementation) => !implementation.js)) {
      await serveAndConnect(kind, server, implementations, dir);
    }
    if (kind === 'stdio') continue;
    const servers = implementations.filter((implementation) => !implementation.js);
    for (const relay of servers) {
      const server = servers.find((other) => other !== relay) ?? relay;
      await relayAndConnect(kind, relay, server, implementations, dir);
    }
  }
} finally {
  fs.rmSync(dir, { recursive: true, force: true });
}
const failed = results.filter((result) => !result.ok);
console.log(`\n${results.length - failed.length} of ${results.length} conversations passed`);
for (const result of failed) console.log(`  failed: ${result.label}`);
process.exitCode = failed.length > 0 ? 1 : 0;
