// What the layers that compose links clients share: `UnionLinks` (many
// networks seen as one) and `RemapLinks` (a network under other addresses).
// They work through the methods of the clients below them, so what they
// cannot translate (LiNo text that names addresses of one store) they refuse
// up front rather than send to the wrong store.

import { GUARANTEES } from './capabilities.js';
import { scopeOf } from './deadline.js';
import { LinksError, UnsupportedError } from './errors.js';
import { ANY } from './operations.js';

/** The operations a composed layer translates. */
export const COMPOSED_OPERATIONS = ['count', 'each', 'create', 'update', 'delete', 'getName', 'setName', 'getByName', 'removeName'];

/** Throws an UnsupportedError unless `capabilities` offer every guarantee named. */
export function requireGuarantees(capabilities, guarantees) {
  const missing = guarantees.filter((guarantee) => {
    if (!GUARANTEES.includes(guarantee)) throw new TypeError(`unknown guarantee "${guarantee}"; known: ${GUARANTEES.join(', ')}`);
    return capabilities[guarantee] !== true;
  });
  if (missing.length > 0) {
    throw new UnsupportedError(`the ${capabilities.backend} backend does not offer ${missing.join(', ')}; nothing is emulated`);
  }
}

/** Whether a restriction part matches any address. */
export function isAny(part) {
  return part === null || part === undefined || part === ANY;
}

/** Checks the shape of a restriction: an array of up to three parts. */
export function checkRestriction(restriction) {
  if (!Array.isArray(restriction)) throw new TypeError('a restriction is an array of up to three parts');
  if (restriction.length > 3) throw new TypeError(`a restriction has at most three parts, got ${restriction.length}`);
  return restriction;
}

/** Orders links by address when every address is a BigInt. */
export function byIndex(links) {
  if (links.every((link) => typeof link.index === 'bigint')) links.sort((a, b) => (a.index < b.index ? -1 : a.index > b.index ? 1 : 0));
  return links;
}

/** A rejection telling that `layer` cannot translate `what`. */
export function untranslatable(layer, what) {
  return new UnsupportedError(
    `${layer} cannot translate ${what}: LiNo text names the addresses of one store; send it to the store below (\`links\` or \`members[i]\`)`,
  );
}

/** Whether a value is the null link. */
export function isNullAddress(value) {
  if (typeof value === 'bigint') return value === 0n;
  if (typeof value === 'number') return value === 0;
  return value === '0';
}

const ARGUMENTS = {
  count: ({ restriction = [] }) => [restriction],
  each: ({ restriction = [] }) => [restriction],
  create: ({ source, target }) => [source, target],
  update: ({ index, source, target }) => [index, source, target],
  delete: ({ index }) => [index],
  getName: ({ index }) => [index],
  setName: ({ index, name }) => [index, name],
  getByName: ({ name }) => [name],
  removeName: ({ index }) => [index],
};

/**
 * The `batch` of a composed layer: the operations run one after the other
 * through the layer's own methods, so each is translated, and stop after the
 * first failure. The result has the shape of `LinksClient.batch`. An
 * operation the layer does not translate (`query`) refuses the whole batch
 * before anything runs.
 */
export async function runBatch(links, operations, { signal, deadline, timeout } = {}) {
  if (!Array.isArray(operations)) throw new TypeError('a batch is an array of operations');
  const calls = operations.map((operation, position) => {
    const argumentsOf = ARGUMENTS[operation?.op];
    if (!argumentsOf) {
      if (operation?.op === 'query') throw untranslatable(links.capabilities.backend, `operation ${position} (query)`);
      throw new TypeError(`operation ${position}: unknown op "${operation?.op}"`);
    }
    return { op: operation.op, args: argumentsOf(operation) };
  });
  const scope = scopeOf({ signal, deadline, timeout });
  const results = calls.map(() => ({ status: 'not-sent' }));
  for (const [position, { op, args }] of calls.entries()) {
    results[position] = { status: 'unknown' };
    try {
      results[position] = { status: 'ok', value: await links[op](...args, scope) };
    } catch (error) {
      results[position] = { status: error instanceof LinksError ? error.outcome : 'unknown', error };
      break;
    }
  }
  const tally = { ok: 0, failed: 0, unknown: 0, 'not-sent': 0 };
  for (const result of results) tally[result.status] += 1;
  return {
    completed: tally.ok === calls.length,
    results,
    ok: tally.ok,
    failed: tally.failed,
    unknown: tally.unknown,
    notSent: tally['not-sent'],
    firstError: results.find((result) => result.error)?.error ?? null,
  };
}
