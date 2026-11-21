// Replays scripts of link operations against a `DoubletsStorage`, printing
// the result of every operation as rust/examples/unit_store_trace.rs does, so
// the database files both leave can be compared byte for byte; and the seeded
// random scripts the differential tests and the fixtures replay.
//
// One operation per line (`#` starts a comment): `create <source> <target>`,
// `point`, `update <index> <source> <target>`, `delete <index>`,
// `ensure <index>` and `get-or-create <source> <target>`. A failed operation
// prints `error <message>` and the replay goes on.
import { DoubletsStorage } from '../../src/doublets-storage.js';
import { AddressType } from '../../src/unit-store-memory.js';

/** `number::<T>`: an operand that fits the address type of `storage`. */
function number(storage, text) {
  if (text === undefined) throw new Error('missing operand');
  if (!/^\+?\d+$/.test(text)) throw new Error(`bad number ${text}`);
  const value = Number(text);
  if (value > storage.store().type.max) {
    throw new Error(`${text} does not fit the address type`);
  }
  return value;
}

const shown = (value) => String(value);
const linkText = (value) => `${value.index} ${value.source} ${value.target}`;

/** The output line of one operation. */
function apply(storage, line) {
  const [operation, ...operands] = line.split(/\s+/);
  const next = () => number(storage, operands.shift());
  const run = (action) => {
    try {
      return action();
    } catch (error) {
      return `error ${error.message}`;
    }
  };
  switch (operation) {
    case 'create': {
      const [source, target] = [next(), next()];
      return run(() => shown(storage.createLink(source, target)));
    }
    case 'point':
      return run(() => shown(storage.store().createPoint()));
    case 'update': {
      const [index, source, target] = [next(), next(), next()];
      return run(() => linkText(storage.updateLink(index, source, target)));
    }
    case 'delete': {
      const index = next();
      return run(() => linkText(storage.deleteLink(index)));
    }
    case 'ensure': {
      const index = next();
      return run(() => shown(storage.ensureLinkCreated(index)));
    }
    case 'get-or-create': {
      const [source, target] = [next(), next()];
      return run(() => shown(storage.getOrCreateLink(source, target)));
    }
    default:
      throw new Error(`unknown operation ${operation}`);
  }
}

/** Replays `script` against `storage`, returning the lines the Rust example prints. */
export function replay(storage, script) {
  const output = [];
  for (const raw of script.split('\n')) {
    const line = raw.split('#')[0].trim();
    if (line !== '') output.push(apply(storage, line));
  }
  output.push(`count ${storage.linksCount()}`);
  storage.flush();
  return output;
}

/** A seeded `mulberry32` generator of numbers in `[0, 1)`. */
export function seededRandom(seed) {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let value = state;
    value = Math.imul(value ^ (value >>> 15), value | 1);
    value ^= value + Math.imul(value ^ (value >>> 7), value | 61);
    return ((value ^ (value >>> 14)) >>> 0) / 4294967296;
  };
}

/**
 * A random script of `count` operations over the addresses created so far
 * (and a few past them), so it reuses freed addresses, hits missing links
 * and builds chains the resolvers cascade through.
 *
 * Without the resolvers a unit store must never hold two links with the same
 * `(source, target)` (the trees cannot tell them apart, and Rust panics on
 * it), so with `unique` every pair a write asks for has one part that is a
 * fresh number no address reaches and the source is never 0 (`search` walks
 * the sources tree only, so `get-or-create` would not find a `(0, target)`
 * link and would create it twice); only `get-or-create` repeats a pair.
 */
export function randomScript(seed, count, { unique = false } = {}) {
  const random = seededRandom(seed);
  let high = 1;
  let fresh = 1_000_000;
  const pairs = [];
  const pick = () => Math.floor(random() * (high + 2));
  const pair = () => {
    if (!unique) return `${pick()} ${pick()}`;
    fresh++;
    const created = random() < 0.5 ? `${Math.max(pick(), 1)} ${fresh}` : `${fresh} ${pick()}`;
    pairs.push(created);
    return created;
  };
  const known = () =>
    unique && pairs.length > 0 && random() < 0.5
      ? pairs[Math.floor(random() * pairs.length)]
      : pair();
  const lines = [];
  for (let step = 0; step < count; step++) {
    const roll = random();
    if (roll < 0.35) {
      lines.push(`create ${pair()}`);
      high++;
    } else if (roll < 0.42) {
      lines.push('point');
      high++;
    } else if (roll < 0.62) lines.push(`update ${pick()} ${pair()}`);
    else if (roll < 0.84) lines.push(`delete ${pick()}`);
    else if (roll < 0.94) lines.push(`get-or-create ${known()}`);
    else lines.push(`ensure ${pick() + Math.floor(random() * 4)}`);
  }
  return `${lines.join('\n')}\n`;
}

/** Opens `database` as the example does and replays `script` against it. */
export function trace(database, script, { resolved = false, u32 = false } = {}) {
  const addressType = u32 ? AddressType.U32 : AddressType.U64;
  let storage = DoubletsStorage.open(database, { addressType });
  if (resolved) storage = storage.withAutomaticUniquenessAndUsagesResolution();
  try {
    return replay(storage, script);
  } finally {
    storage.close();
  }
}
