// An in-memory links network behind the async links contract, for the tests
// of the composing layers: links stay apart when equal (as the Rust store),
// a delete takes the links that use the link with it, and every address is
// checked against the u32 range of the real backends.

import { AddressRangeError, RemoteError, toAddress, wasmCapabilities } from '../src/index.js';

export class MemoryLinks {
  links = new Map();
  names = new Map();
  closed = 0;

  constructor(name = 'memory') {
    this.capabilities = wasmCapabilities({ backend: name });
  }

  require() {
    return this;
  }

  #address(value, what) {
    const address = toAddress(value, what);
    if (address > this.capabilities.maxAddress) throw new AddressRangeError(`${what} ${address} exceeds ${this.capabilities.maxAddress}`);
    return address;
  }

  #link(index) {
    const link = this.links.get(index);
    return link ? { index, ...link } : null;
  }

  #matches(link, restriction) {
    const parts = restriction.map((part, at) => (part === null || part === undefined || part === '*' ? null : this.#address(part, `part ${at}`)));
    const [index, first, second] = parts;
    if (index !== null && index !== undefined && link.index !== index) return false;
    if (parts.length === 2) return first === null || link.source === first || link.target === first;
    if (first !== null && first !== undefined && link.source !== first) return false;
    return second === null || second === undefined || link.target === second;
  }

  async count(restriction = []) {
    return BigInt((await this.each(restriction)).length);
  }

  async each(restriction = []) {
    return [...this.links.keys()].sort((a, b) => (a < b ? -1 : 1)).map((index) => this.#link(index)).filter((link) => this.#matches(link, restriction));
  }

  async create(source, target) {
    const link = { source: this.#address(source, 'source'), target: this.#address(target, 'target') };
    let index = 1n;
    while (this.links.has(index)) index += 1n;
    this.links.set(index, link);
    return this.#link(index);
  }

  async update(index, source, target) {
    const address = this.#address(index, 'index');
    const before = this.#link(address);
    if (!before) throw new RemoteError(`Link not found: ${address}`);
    this.links.set(address, { source: this.#address(source, 'source'), target: this.#address(target, 'target') });
    return [{ before, after: this.#link(address) }];
  }

  async delete(index) {
    const address = this.#address(index, 'index');
    const deleted = this.#link(address);
    if (!deleted) throw new RemoteError(`Link not found: ${address}`);
    this.links.delete(address);
    const changes = [];
    for (const usage of await this.each([])) {
      if (this.links.has(usage.index) && (usage.source === address || usage.target === address)) changes.push(...(await this.delete(usage.index)));
    }
    changes.push({ before: deleted, after: null });
    await this.removeName(address);
    return changes;
  }

  async getName(index) {
    const address = this.#address(index, 'index');
    for (const [name, named] of this.names) if (named === address) return name;
    return null;
  }

  async setName(index, name) {
    const address = this.#address(index, 'index');
    await this.removeName(address);
    this.names.set(name, address);
    return address;
  }

  async getByName(name) {
    return this.names.get(name) ?? null;
  }

  async removeName(index) {
    const address = this.#address(index, 'index');
    for (const [name, named] of this.names) if (named === address) this.names.delete(name);
  }

  async close() {
    this.closed += 1;
  }
}

/** A new link `(source target)`, made as a client makes one: created, then updated. */
export async function make(links, source, target) {
  const { index } = await links.create(0n, 0n);
  await links.update(index, source, target);
  return index;
}

/** Every link matching `restriction` as `[index, source, target]`. */
export async function all(links, restriction = []) {
  return (await links.each(restriction)).map(({ index, source, target }) => [index, source, target]);
}
