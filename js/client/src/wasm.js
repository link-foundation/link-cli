// The in-process WebAssembly backend: the `Clink` store of `rust/wasm`
// (`clink-wasm`), answering the same LinksOperation documents as
// `clink --serve`. It runs in browsers, Node.js, Bun and Deno; it holds its
// links in memory and opens no socket.

import { wasmCapabilities } from './capabilities.js';
import { LinksClient } from './client.js';
import { scopeError } from './deadline.js';
import { ConnectionClosedError, LimitExceededError, LinksError, UnsupportedError } from './errors.js';
import { DEFAULT_MAX_MESSAGE_BYTES } from './framing.js';

const encoder = new TextEncoder();

/**
 * Adapts a `Clink` instance (anything with `request(text) => text`) to the
 * transport contract. Requests run one at a time, synchronously, on the
 * calling thread: a request that started cannot be aborted, so `signal` and
 * `deadline` are checked before it starts.
 */
export class WasmTransport {
  #clink;
  #owned;
  #closed = false;
  #tail = Promise.resolve();

  constructor(clink, { owned = false, maxRequestBytes = DEFAULT_MAX_MESSAGE_BYTES, maxReplyBytes = DEFAULT_MAX_MESSAGE_BYTES, capabilities } = {}) {
    if (!clink || typeof clink.request !== 'function') {
      throw new TypeError('expected a Clink instance with request(text); build rust/wasm with a version that has it');
    }
    this.#clink = clink;
    this.#owned = owned;
    this.maxRequestBytes = maxRequestBytes;
    this.maxReplyBytes = maxReplyBytes;
    this.capabilities = wasmCapabilities({ ...capabilities, limits: { maxRequestBytes, maxReplyBytes } });
    this.stats = { requests: 0, bytesSent: 0, bytesReceived: 0 };
  }

  send(text, scope = {}) {
    const run = () => {
      if (this.#closed) throw new ConnectionClosedError('the WebAssembly store is closed', { outcome: 'not-sent' });
      const size = encoder.encode(text).length;
      if (size > this.maxRequestBytes) {
        throw new LimitExceededError(`request of ${size} bytes exceeds maxRequestBytes ${this.maxRequestBytes}`, { outcome: 'not-sent' });
      }
      const late = scopeError(scope, 'not-sent');
      if (late) throw late;
      this.stats.requests += 1;
      this.stats.bytesSent += size;
      const reply = this.#clink.request(text);
      const replySize = encoder.encode(reply).length;
      this.stats.bytesReceived += replySize;
      if (replySize > this.maxReplyBytes) {
        // It ran; only its reply is dropped.
        throw new LimitExceededError(`reply of ${replySize} bytes exceeds maxReplyBytes ${this.maxReplyBytes}`, { outcome: 'unknown' });
      }
      return reply;
    };
    // Keep requests in order, each after the previous one settled.
    return this.#enqueue(run);
  }

  #enqueue(run) {
    const result = this.#tail.then(run);
    this.#tail = result.catch(() => {});
    return result;
  }

  /** Serializes transaction controls after all previously queued requests. */
  control(method) {
    return this.#enqueue(() => {
      if (this.#closed) throw new ConnectionClosedError('the WebAssembly store is closed', { outcome: 'not-sent' });
      if (!['commit', 'rollback'].includes(method) || typeof this.#clink[method] !== 'function') {
        throw new UnsupportedError('this WebAssembly store has no transaction control');
      }
      try { return this.#clink[method](); }
      catch (cause) { throw new LinksError(String(cause), {
        outcome: method === 'commit' && String(cause).includes('indeterminate commit') ? 'unknown' : 'failed', cause,
      }); }
    });
  }

  /** Waits for queued requests; frees the store only if this transport owns it. */
  async close() {
    await this.#tail;
    if (this.#closed) return;
    this.#closed = true;
    if (this.#owned) this.#clink.free?.();
  }
}

/**
 * A {@link LinksClient} over a `Clink` store. Pass `{ owned: true }` to let
 * `client.close()` free the store; by default its creator keeps it.
 */
export function createWasmClient(clink, options = {}) {
  return new LinksClient(new WasmTransport(clink, options));
}
