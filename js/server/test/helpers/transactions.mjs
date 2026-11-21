// What the transactions and version-control tests share: the in-memory log
// of rust/src/engine (`MemoryLog`), named in-memory stores, the shared
// protocol vectors of docs/protocol and a fault-injecting proxy standing in
// for the failing adapters of the Rust integration tests.
import { readFileSync } from 'node:fs';
import { LinkStorage } from '../../src/link-storage.js';
import { NamedTypesDecorator } from '../../src/named-types.js';
import { TransactionsDecorator } from '../../src/transactions.js';
import { VersionControlDecorator } from '../../src/version-control.js';

/** A transitions log kept in `entries`, as the Rust `MemoryLog`. */
export function memoryLog(entries = []) {
  return {
    entries,
    appendLogEntry(entry) {
      this.entries.push(entry);
    },
    readLogEntries() {
      return [...this.entries];
    },
    flushLog() {},
  };
}

/** A names-capable in-memory store: a `NamedTypesDecorator` over two in-memory stores. */
export const namedMemory = () =>
  new NamedTypesDecorator(LinkStorage.inMemory(), LinkStorage.inMemory());

/** A transactions decorator over `store` (a plain in-memory store by default). */
export const openTx = ({
  store = LinkStorage.inMemory(),
  log = memoryLog(),
  retention,
  mode,
  trace = false,
  stderr,
} = {}) => new TransactionsDecorator(store, log, retention, mode, trace, { stderr });

/** A version-control decorator over in-memory stores. */
export function openVc({ store = namedMemory(), log = memoryLog(), branches, trace, stderr } = {}) {
  const tx = openTx({ store, log, trace, stderr });
  return new VersionControlDecorator(tx, branches ?? namedMemory(), trace, { stderr });
}

/**
 * The vectors of `docs/protocol/<file>`. Integers too large for a double
 * are read as strings; `BigInt(value)` reads either form.
 */
export function protocolVectors(file) {
  const text = readFileSync(new URL(`../../../../docs/protocol/${file}`, import.meta.url), 'utf8');
  return JSON.parse(text.replace(/(:\s*)(-?\d{16,})(?=\s*[,}\]])/g, '$1"$2"'));
}

/**
 * `target` behind a proxy whose methods can be made to throw:
 * `proxy.fail(method, { after, times, error })` skips `after` calls and then
 * fails `times` calls (`Infinity` for every call); `proxy.heal()` clears the
 * plan. Calls between the target's own methods are not intercepted.
 */
export function faulty(target) {
  const plan = new Map();
  const proxy = new Proxy(target, {
    get(object, key) {
      if (key === 'fail') {
        return (method, { after = 0, times = 1, error } = {}) => {
          plan.set(method, { after, times, error: error ?? new Error(`injected ${method}`) });
          return proxy;
        };
      }
      if (key === 'heal') return () => plan.clear();
      const value = Reflect.get(object, key, object);
      if (typeof value !== 'function') return value;
      return (...args) => {
        const fault = plan.get(key);
        if (fault !== undefined) {
          if (fault.after > 0) fault.after--;
          else if (fault.times > 0) {
            fault.times--;
            throw fault.error;
          }
        }
        return value.apply(object, args);
      };
    },
  });
  return proxy;
}

/** The entries of `log` starting with `prefix`. */
export const entriesWith = (log, prefix) => log.entries.filter((entry) => entry.startsWith(prefix));
