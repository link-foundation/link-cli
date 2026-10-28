// Selected owned snapshot projection. Performs no I/O or publication.
import { CausalVersion } from './engine-causal-version.js';

const invalid = () => { throw new TypeError('invalid causal projection'); };
const own = value => CausalVersion.decode(CausalVersion.encode(value));
const byAddress = (left, right) => BigInt(left[0]) < BigInt(right[0]) ? -1 : 1;
export class CausalProjection {
  #retained; #selected;
  constructor(values, policy, options = {}) {
    if (options === null || typeof options !== 'object' || Array.isArray(options)) invalid();
    const { maxVersions = 4096, maxBytes = 8388608 } = options;
    if (!['reject', 'max-dot'].includes(policy) || !Number.isInteger(maxVersions) ||
        maxVersions < 1 || maxVersions > 4096 || !Number.isInteger(maxBytes) ||
        maxBytes < 1 || maxBytes > 8388608 || !Array.isArray(values) || values.length > maxVersions) invalid();
    const groups = new Map(), dots = new Map();
    let bytes = 0;
    for (const input of values) {
      const text = CausalVersion.encode(input);
      if ((bytes += text.length) > maxBytes) invalid();
      const value = CausalVersion.decode(text), dot = `${value.dot.author}:${value.dot.counter}`;
      if (dots.has(dot) && dots.get(dot) !== text) invalid();
      dots.set(dot, text);
      if (!groups.has(value.address)) groups.set(value.address, []);
      const group = groups.get(value.address);
      if (group.length === 64) invalid();
      group.push(value);
    }
    this.#retained = []; this.#selected = [];
    for (const [, group] of [...groups].sort(byAddress)) {
      const siblings = CausalVersion.merge(group, []);
      if (policy === 'reject' && siblings.length !== 1) throw new Error('conflict: concurrent causal siblings');
      this.#retained.push(...siblings);
      this.#selected.push(siblings.at(-1));
    }
  }
  versions() { return this.#retained.map(own); }
  selected() { return this.#selected.map(own); }
  rows() {
    return this.#selected.filter(value => value.payload !== null).map(value =>
      ({ index: value.address, source: value.payload.source, target: value.payload.target, name: value.payload.name }));
  }
}
