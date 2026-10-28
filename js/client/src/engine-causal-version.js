// Selected causal semantics only. No I/O, ownership fencing or replication.
const maximum = 18446744073709551615n;
const utf8 = new TextEncoder();
const text = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true });
const invalid = () => { throw new TypeError('invalid causal version'); };
const array = (value, size) => {
  if (!Array.isArray(value) || value.length !== size) invalid();
};
const counter = (value, positive = true) => {
  if (typeof value !== 'string' || value.length > 20 || !/^(0|[1-9][0-9]*)$/.test(value) ||
      BigInt(value) > maximum || (positive && value === '0')) invalid();
  return BigInt(value);
};
const author = value => {
  if (typeof value !== 'string' || !/^[0-9a-f]{32}$/.test(value) || value === '0'.repeat(32)) invalid();
  return value;
};
const object = value => {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) invalid();
};
const payload = value => {
  if (value === null) return null;
  object(value); counter(value.source, false); counter(value.target, false);
  let name = null;
  if (value.name !== null) {
    if (typeof value.name !== 'string' || value.name.length > 4096 || !value.name.isWellFormed()) invalid();
    const bytes = utf8.encode(value.name);
    if (bytes.length > 4096) invalid();
    name = Array.from(bytes, b => b.toString(16).padStart(2, '0')).join('');
  }
  return [value.source, value.target, name];
};
const normalize = value => CausalVersion.decode(CausalVersion.encode(value));
const versions = values => {
  if (!Array.isArray(values) || values.length > 64) invalid();
  return Array.from(values, normalize);
};
const dominates = (left, right) => {
  const clock = new Map(left.clock.map(item => [item.author, BigInt(item.counter)]));
  return right.clock.every(item => (clock.get(item.author) ?? 0n) >= BigInt(item.counter));
};
const compare = (a, b) => a.dot.author < b.dot.author ? -1 : a.dot.author > b.dot.author ? 1 :
  BigInt(a.dot.counter) < BigInt(b.dot.counter) ? -1 : BigInt(a.dot.counter) > BigInt(b.dot.counter) ? 1 : 0;

export class CausalVersion {
  static encode(value) {
    object(value); counter(value.address); object(value.dot);
    author(value.dot.author); counter(value.dot.counter);
    if (!Array.isArray(value.clock) || value.clock.length < 1 || value.clock.length > 64) invalid();
    let previous = '', found = false;
    const clock = Array.from(value.clock, item => {
      object(item); author(item.author); counter(item.counter);
      if (item.author <= previous) invalid(); previous = item.author;
      if (item.author === value.dot.author) {
        if (item.counter !== value.dot.counter) invalid(); found = true;
      }
      return [item.author, item.counter];
    });
    if (!found) invalid();
    return JSON.stringify(['causal-version1', value.address,
      [value.dot.author, value.dot.counter], clock, payload(value.payload)]);
  }
  static decode(wire) {
    if (typeof wire !== 'string' || wire.length > 65536 || !/^[\x20-\x7e]*$/.test(wire)) invalid();
    const value = JSON.parse(wire); array(value, 5);
    if (value[0] !== 'causal-version1') invalid(); array(value[2], 2);
    if (!Array.isArray(value[3]) || value[3].length < 1 || value[3].length > 64) invalid();
    const clock = value[3].map(item => { array(item, 2); return { author: item[0], counter: item[1] }; });
    let row = null;
    if (value[4] !== null) {
      array(value[4], 3); let name = value[4][2];
      if (name !== null) {
        if (typeof name !== 'string' || name.length > 8192 || !/^(?:[0-9a-f]{2})*$/.test(name)) invalid();
        name = text.decode(Uint8Array.from(name.match(/../g) ?? [], byte => parseInt(byte, 16)));
      }
      row = { source: value[4][0], target: value[4][1], name };
    }
    const result = { address: value[1], dot: { author: value[2][0], counter: value[2][1] }, clock, payload: row };
    if (CausalVersion.encode(result) !== wire) invalid();
    return result;
  }
  static merge(left, right) {
    const values = [...versions(left), ...versions(right)], unique = [];
    const address = values[0]?.address;
    for (const item of values) {
      if (item.address !== address) invalid();
      let duplicate = false;
      for (const previous of unique) {
        const sameDot = compare(previous, item) === 0;
        const sameClock = JSON.stringify(previous.clock) === JSON.stringify(item.clock);
        if (sameDot || sameClock) {
          if (CausalVersion.encode(previous) !== CausalVersion.encode(item)) invalid();
          duplicate = true;
        }
      }
      if (!duplicate) unique.push(item);
    }
    const result = unique.filter(item => !unique.some(other => other !== item && dominates(other, item)));
    if (result.length > 64) invalid();
    return result.sort(compare);
  }
  static advance(address, localAuthor, lastCounter, observed, row) {
    counter(address); author(localAuthor); let next = counter(lastCounter, false);
    const values = versions(observed), clock = new Map();
    // Validate contradictions before constructing the observed context.
    CausalVersion.merge(values, []);
    for (const value of values) {
      if (value.address !== address) invalid();
      for (const item of value.clock) {
        const count = BigInt(item.counter);
        if (count > (clock.get(item.author) ?? 0n)) clock.set(item.author, count);
      }
    }
    if ((clock.get(localAuthor) ?? 0n) > next) next = clock.get(localAuthor);
    if (next === maximum) invalid(); next++;
    clock.set(localAuthor, next);
    return normalize({ address, dot: { author: localAuthor, counter: next.toString() },
      clock: [...clock].sort(([a], [b]) => a < b ? -1 : 1)
        .map(([id, count]) => ({ author: id, counter: count.toString() })), payload: row });
  }
}
