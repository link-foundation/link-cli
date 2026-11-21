// The rendezvous directory of the file-based kinds (`fifo`, `file`, `mmap`,
// `shm`, `signal`; the "Rendezvous" section of
// docs/protocol/ipc-transports-v1.md): a port of
// rust/src/protocol/ipc/rendezvous.rs, adapted from
// js/client/src/ipc-rendezvous.js.
//
// A listener owns `DIR/listener.json`; a client writes `DIR/{id}.req`, and the
// listener answers with `DIR/{id}.ack`. Rust sleeps a thread between polls;
// here the waits are timers, and the file operations stay synchronous as in
// Rust. Rust's `Drop` cleanups are `close()` and `finish()`.
import * as fs from 'node:fs';
import { join } from 'node:path';
import { errorText } from './error-report.js';
import { IoError } from './protocol-errors.js';
import { trace } from './ipc-trace.js';
import { JsonObject } from './ipc-json.js';
import { processAlive, randomId, sleep } from './ipc-util.js';

const VERSION = 1n;
const LISTENER = 'listener.json';
const OWNED_SUFFIXES = ['.req', '.ack', '.tmp', '.c2s', '.s2c', '.mmap', '.ring', '.sig'];

/** `Backoff`: sleeps from 1 ms, doubling up to `limit` (at least 50 µs). */
export class Backoff {
  constructor(limit) {
    this.limit = Math.max(limit, 0.05);
    this.current = Math.min(1, this.limit);
  }

  async sleep() {
    await sleep(this.current);
    this.current = Math.min(this.current * 2, this.limit);
  }
}

/** `write_atomically`: writes `path.tmp`, then renames it over `path`. */
export function writeAtomically(path, text) {
  const temporary = `${path}.tmp`;
  fs.writeFileSync(temporary, text);
  try {
    fs.renameSync(temporary, path);
  } catch (error) {
    fs.rmSync(temporary, { force: true });
    throw error;
  }
}

/** `fs::read_to_string`: text that is not UTF-8 fails with `InvalidData`. */
export function readText(path) {
  const bytes = fs.readFileSync(path);
  try {
    return new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(bytes);
  } catch {
    throw new IoError('InvalidData', 'stream did not contain valid UTF-8');
  }
}

const readObject = (path) => JsonObject.parse(readText(path));

/** `canonical_kind`: `pipe:` on POSIX is `fifo:`. */
export const canonicalKind = (kind) => (kind === 'pipe' ? 'fifo' : kind);

const asPid = (number) => Number(BigInt.asUintN(32, number));

const validId = (id) => /^[0-9a-f]{20}$/.test(id);

const refused = (message) => new IoError('ConnectionRefused', message);

/** `Request`: one client's request, answered by `ack` or `refuse`. */
export class Request {
  #answered = false;

  constructor(id, fields, dir) {
    this.id = id;
    this.fields = fields;
    this.dir = dir;
  }

  /** The client's process id, 0 when missing. */
  pid() {
    return asPid(this.fields.number('pid') ?? 0n);
  }

  /** `DIR/{id}.{suffix}`. */
  path(suffix) {
    return join(this.dir, `${this.id}.${suffix}`);
  }

  /** Accepts with `fields`, adding the version and this process's id. */
  ack(fields) {
    this.#answered = true;
    writeAtomically(this.path('ack'), fields.with('v', VERSION).with('pid', process.pid).toJson());
  }

  /** Refuses with `error`; a failure to answer is ignored. */
  refuse(error) {
    this.#answered = true;
    try {
      writeAtomically(
        this.path('ack'),
        new JsonObject().with('v', VERSION).with('error', error).toJson(),
      );
    } catch {
      // The client times out instead.
    }
  }

  /** Rust's `Drop`: an unanswered request is refused. */
  finish() {
    if (!this.#answered) this.refuse('the listener could not set the connection up');
  }
}

/** `RendezvousListener`: owns a rendezvous directory and reads its requests. */
export class RendezvousListener {
  #closed = false;
  #wake = null;

  constructor(dir, kind, acceptPoll, endpoint) {
    this.dir = dir;
    this.kind = kind;
    this.acceptPoll = acceptPoll;
    this.endpointText = endpoint;
  }

  /** `bind`: creates the directory and claims it, removing stale files. */
  static bind(endpoint, kind) {
    const dir = endpoint.address;
    fs.mkdirSync(dir, { recursive: true, mode: 0o700 });
    const listener = join(dir, LISTENER);
    let existing = null;
    try {
      existing = readObject(listener);
    } catch {
      // No listener yet.
    }
    const pid = existing?.number('pid');
    if (pid !== undefined && processAlive(asPid(pid))) {
      throw new IoError('AddrInUse', `process ${pid} already listens on \`${dir}\``);
    }
    let names = [];
    try {
      names = fs.readdirSync(dir);
    } catch {
      // Nothing to remove.
    }
    for (const name of names) {
      if (name === LISTENER || OWNED_SUFFIXES.some((suffix) => name.endsWith(suffix))) {
        fs.rmSync(join(dir, name), { force: true });
      }
    }
    writeAtomically(
      listener,
      new JsonObject()
        .with('v', VERSION)
        .with('kind', canonicalKind(kind))
        .with('pid', process.pid)
        .toJson(),
    );
    return new RendezvousListener(dir, kind, endpoint.millis('accept-poll', 20), String(endpoint));
  }

  endpoint() {
    return this.endpointText;
  }

  get closed() {
    return this.#closed;
  }

  /** The next well-formed request, or `null` once closed. */
  async next() {
    const backoff = new Backoff(this.acceptPoll);
    for (;;) {
      if (this.#closed) return null;
      const names = fs
        .readdirSync(this.dir)
        .filter((name) => name.endsWith('.req'))
        .sort();
      for (const name of names) {
        const request = this.#take(name);
        if (request !== null) return request;
      }
      await Promise.race([backoff.sleep(), new Promise((resolve) => (this.#wake = resolve))]);
    }
  }

  #take(name) {
    const path = join(this.dir, name);
    let text;
    try {
      text = readText(path);
      fs.unlinkSync(path);
    } catch {
      return null;
    }
    const request = new Request(name.slice(0, -'.req'.length), new JsonObject(), this.dir);
    let fields = null;
    try {
      fields = JsonObject.parse(text);
    } catch {
      // Malformed below.
    }
    if (
      fields !== null &&
      fields.number('v') === VERSION &&
      fields.text('kind') === canonicalKind(this.kind) &&
      fields.number('pid') !== undefined &&
      validId(request.id)
    ) {
      request.fields = fields;
      trace(`${this.kind} request ${request.id} from process ${request.pid()}`);
      return request;
    }
    trace(`${this.kind} refused malformed request ${name}`);
    request.refuse('malformed or mismatched request');
    return null;
  }

  /** The closer and `Drop`: stops `next` and releases the directory. */
  close() {
    if (this.#closed) return;
    this.#closed = true;
    this.#wake?.();
    const listener = join(this.dir, LISTENER);
    try {
      if (readObject(listener).number('pid') === BigInt(process.pid)) fs.unlinkSync(listener);
    } catch {
      // Not ours, or gone.
    }
    try {
      fs.rmdirSync(this.dir);
    } catch {
      // Not empty.
    }
  }
}

/** `Rendezvous`: a client's side of one connection set-up. */
export class Rendezvous {
  constructor(dir, id, serverPid, kind, deadline, poll) {
    this.dir = dir;
    this.id = id;
    this.serverPid = serverPid;
    this.kind = kind;
    this.deadline = deadline;
    this.poll = poll;
  }

  /** `begin`: checks that a live listener of `kind` owns the directory. */
  static begin(endpoint, kind, alive = processAlive) {
    const dir = endpoint.address;
    let listener;
    try {
      listener = readObject(join(dir, LISTENER));
    } catch (error) {
      throw refused(`nobody listens on \`${dir}\`: ${errorText(error)}`);
    }
    const pid = asPid(listener.number('pid') ?? 0n);
    if (listener.number('v') !== VERSION) {
      throw refused(`\`${dir}\` uses another rendezvous version`);
    }
    const found = listener.text('kind') ?? '';
    if (found !== canonicalKind(kind)) {
      throw refused(`\`${dir}\` listens for \`${found}:\` connections, not \`${kind}:\``);
    }
    if (pid === 0 || !alive(pid)) {
      throw refused(`the listener of \`${dir}\` (process ${pid}) is gone`);
    }
    const deadline = performance.now() + endpoint.timeout();
    return new Rendezvous(dir, randomId(), pid, kind, deadline, endpoint.millis('poll', 5));
  }

  /** `DIR/{id}.{suffix}`. */
  path(suffix) {
    return join(this.dir, `${this.id}.${suffix}`);
  }

  /** Writes the request and waits for the listener's answer. */
  async request(fields, alive = processAlive) {
    const request = this.path('req');
    const ack = this.path('ack');
    writeAtomically(
      request,
      fields
        .with('v', VERSION)
        .with('kind', canonicalKind(this.kind))
        .with('pid', process.pid)
        .toJson(),
    );
    trace(`${this.kind} request ${this.id} written`);
    const backoff = new Backoff(Math.max(this.poll, 1));
    let lastCheck = performance.now();
    for (;;) {
      let text = '';
      try {
        text = readText(ack);
      } catch {
        // Not answered yet.
      }
      if (text !== '') {
        fs.rmSync(ack, { force: true });
        const object = JsonObject.parse(text);
        const error = object.text('error');
        if (error !== undefined) throw refused(`the listener refused: ${error}`);
        return object;
      }
      const now = performance.now();
      let gone = false;
      if (now - lastCheck >= 100) {
        lastCheck = now;
        gone = !alive(this.serverPid);
      }
      if (gone || now >= this.deadline) {
        fs.rmSync(request, { force: true });
        fs.rmSync(ack, { force: true });
        throw gone
          ? refused(`the listener (process ${this.serverPid}) is gone`)
          : new IoError('TimedOut', `no answer from the listener of \`${this.dir}\` in time`);
      }
      await backoff.sleep();
    }
  }
}

/**
 * `accept_with`: answers requests until `open(request)` sets one up; it
 * resolves to `{fields, stream}`. A failure refuses that request only.
 */
export async function acceptWith(rendezvous, kind, open) {
  for (;;) {
    const request = await rendezvous.next();
    if (request === null) return null;
    try {
      let opened;
      try {
        opened = await open(request);
      } catch (error) {
        trace(`${kind} refused ${request.id}: ${errorText(error)}`);
        request.refuse(errorText(error));
        continue;
      }
      try {
        request.ack(opened.fields);
      } catch (error) {
        opened.stream.shutdown();
        throw error;
      }
      trace(`${kind} accepted ${request.id} from process ${request.pid()}`);
      return opened.stream;
    } finally {
      request.finish();
    }
  }
}
