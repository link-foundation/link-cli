// The client side of the rendezvous directory where the local kinds meet
// (the "Rendezvous directory" section of docs/protocol/ipc-transports-v1.md;
// the reference is rust/src/protocol/ipc/rendezvous.rs).

import { randomBytes } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { IpcError } from './ipc-endpoint.js';
import { trace } from './ipc-trace.js';

const VERSION = 1;
const PEER_CHECK = 100;

/** Whether the process `pid` still runs (a process of another user counts). */
export function processAlive(pid) {
  if (!Number.isInteger(pid) || pid <= 0 || pid > 0x7fffffff) return false;
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return error.code === 'EPERM';
  }
}

/** Watches whether a peer process is alive, at most every 100 ms. */
export class PeerWatch {
  #checked = Date.now();
  #alive = true;

  constructor(pid) {
    this.pid = pid;
  }

  alive() {
    if (this.#alive && Date.now() - this.#checked >= PEER_CHECK) {
      this.#alive = processAlive(this.pid);
      this.#checked = Date.now();
    }
    return this.#alive;
  }
}

/** A fresh identifier of 20 lowercase hexadecimal digits. */
export const randomId = () => randomBytes(10).toString('hex');

/** JSON with the keys sorted by byte, as the Rust side writes it. */
export function toJson(fields) {
  const keys = Object.keys(fields).sort((a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b)));
  return `{${keys.map((key) => `${JSON.stringify(key)}:${JSON.stringify(fields[key])}`).join(',')}}`;
}

/** A whole number field of a rendezvous object, or `undefined`. */
export function numberField(object, name) {
  const value = object?.[name];
  return Number.isSafeInteger(value) && value >= 0 ? value : undefined;
}

const textField = (object, name) => (typeof object?.[name] === 'string' ? object[name] : undefined);

function parseObject(text) {
  const object = JSON.parse(text);
  if (object === null || typeof object !== 'object' || Array.isArray(object)) {
    throw new IpcError('a rendezvous file must hold a JSON object', 'EINVAL');
  }
  return object;
}

function writeAtomically(file, text) {
  const temporary = `${file}.tmp`;
  fs.writeFileSync(temporary, text);
  try {
    fs.renameSync(temporary, file);
  } catch (error) {
    fs.rmSync(temporary, { force: true });
    throw error;
  }
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const refused = (message) => new IpcError(message, 'ECONNREFUSED');
const canonicalKind = (kind) => (kind === 'pipe' ? 'fifo' : kind);

/** One connection attempt through a rendezvous directory. */
export class Rendezvous {
  constructor(dir, id, serverPid, kind, deadline, poll) {
    this.dir = dir;
    this.id = id;
    this.serverPid = serverPid;
    this.kind = kind;
    this.deadline = deadline;
    this.poll = poll;
  }

  /** Reads `listener.json` of the endpoint's directory and checks it. */
  static begin(endpoint, kind) {
    const dir = endpoint.address;
    let listener;
    try {
      listener = parseObject(fs.readFileSync(path.join(dir, 'listener.json'), 'utf8'));
    } catch (error) {
      throw refused(`nobody listens on \`${dir}\`: ${error.message}`);
    }
    const pid = numberField(listener, 'pid') ?? 0;
    if (numberField(listener, 'v') !== VERSION) {
      throw refused(`\`${dir}\` uses another rendezvous version`);
    }
    const found = textField(listener, 'kind') ?? '';
    if (found !== canonicalKind(kind)) {
      throw refused(`\`${dir}\` listens for \`${found}:\` connections, not \`${kind}:\``);
    }
    if (pid === 0 || !processAlive(pid)) {
      throw refused(`the listener of \`${dir}\` (process ${pid}) is gone`);
    }
    const poll = endpoint.millis('poll', 5);
    return new Rendezvous(dir, randomId(), pid, kind, Date.now() + endpoint.timeout(), poll);
  }

  path(suffix) {
    return path.join(this.dir, `${this.id}.${suffix}`);
  }

  /** Writes the request with `fields` and resolves to the listener's ack. */
  async request(fields = {}, signal) {
    const request = this.path('req');
    const ack = this.path('ack');
    writeAtomically(
      request,
      toJson({ ...fields, v: VERSION, kind: canonicalKind(this.kind), pid: process.pid }),
    );
    trace(`${this.kind} request ${this.id} written`);
    const limit = Math.max(this.poll, 1);
    let backoff = Math.min(1, limit);
    let lastCheck = Date.now();
    const forget = () => {
      fs.rmSync(request, { force: true });
      fs.rmSync(ack, { force: true });
    };
    for (;;) {
      let text = '';
      try {
        text = fs.readFileSync(ack, 'utf8');
      } catch {
        // Not answered yet.
      }
      if (text !== '') {
        fs.rmSync(ack, { force: true });
        const object = parseObject(text);
        const error = textField(object, 'error');
        if (error !== undefined) throw refused(`the listener refused: ${error}`);
        return object;
      }
      if (signal?.aborted) {
        forget();
        throw signal.reason;
      }
      const now = Date.now();
      let gone = false;
      if (now - lastCheck >= PEER_CHECK) {
        lastCheck = now;
        gone = !processAlive(this.serverPid);
      }
      if (gone) {
        forget();
        throw refused(`the listener (process ${this.serverPid}) is gone`);
      }
      if (now >= this.deadline) {
        forget();
        throw new IpcError(`no answer from the listener of \`${this.dir}\` in time`, 'ETIMEDOUT');
      }
      await sleep(backoff);
      backoff = Math.min(backoff * 2, limit);
    }
  }

  /** The peer process of an ack: its `pid`, or the listener's. */
  peerOf(ack) {
    return numberField(ack, 'pid') ?? this.serverPid;
  }
}
