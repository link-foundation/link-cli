// Optional snapshot comparison; the bare engine allocates no tree or hash state.
import { CausalVersion } from './engine-causal-version.js';
const encoder = new TextEncoder(), domain = encoder.encode('causal-merkle1\0');
const invalid = () => { throw new TypeError('invalid causal Merkle state'); };
const owned = value => CausalVersion.decode(CausalVersion.encode(value));
const object = value => { if (value === null || typeof value !== 'object' || Array.isArray(value)) invalid(); };
const integer = (value, max) => {
  if (!Number.isSafeInteger(value) || value < 1 || value > max) invalid(); return value;
};
const address = value => {
  if (typeof value !== 'string' || value.length > 20 || !/^[1-9][0-9]*$/.test(value) ||
      BigInt(value) > 18446744073709551615n) invalid();
  return BigInt(value).toString(16).padStart(16, '0');
};
const prefix = value => {
  if (typeof value !== 'string' || value.length > 15 || !/^[0-9a-f]*$/.test(value)) invalid(); return value;
};
const hash = value => {
  if (typeof value !== 'string' || !/^[0-9a-f]{64}$/.test(value)) invalid(); return value;
};
const hex = bytes => Array.from(bytes, value => value.toString(16).padStart(2, '0')).join('');
const unhex = value => Uint8Array.from(value.match(/../g), item => parseInt(item, 16));
const concat = (...parts) => {
  const result = new Uint8Array(parts.reduce((total, part) => total + part.length, 0));
  let offset = 0; for (const part of parts) { result.set(part, offset); offset += part.length; } return result;
};
const digest = async bytes => hex(new Uint8Array(await globalThis.crypto.subtle.digest('SHA-256', bytes)));
const branch = (depth, children) => digest(concat(domain, new Uint8Array([78, depth]), ...children.map(unhex)));
let emptyPromise;
const empties = () => emptyPromise ??= (async () => {
  const result = new Array(17); result[16] = await digest(concat(domain, new Uint8Array([69])));
  for (let depth = 15; depth >= 0; depth--) result[depth] = await branch(depth, Array(16).fill(result[depth + 1]));
  return result;
})();
const numberBytes = (number, length) => {
  let value = BigInt(number); const bytes = new Uint8Array(length);
  for (let i = length - 1; i >= 0; i--) { bytes[i] = Number(value & 255n); value >>= 8n; } return bytes;
};
const leafHash = async (key, versions, empty) => {
  if (versions.length === 0) return empty[16];
  const fields = versions.flatMap(value => {
    const wire = encoder.encode(CausalVersion.encode(value)); return [numberBytes(wire.length, 4), wire];
  });
  return digest(concat(domain, new Uint8Array([76]), numberBytes(key, 8), numberBytes(versions.length, 2), ...fields));
};
const proof = (value, size) => {
  if (!Array.isArray(value) || value.length !== size) invalid();
  return Array.from(value, level => {
    if (!Array.isArray(level) || level.length !== 15) invalid(); return Array.from(level, hash);
  });
};
const fold = async (current, path, siblings) => {
  for (let depth = path.length - 1; depth >= 0; depth--) {
    const children = siblings[depth].slice(); children.splice(parseInt(path[depth], 16), 0, current);
    current = await branch(depth, children);
  }
  return current;
};
const admission = (values, maxVersions, maxBytes) => {
  if (!Array.isArray(values) || values.length > maxVersions) invalid();
  let bytes = 0; const result = [], dots = new Map(), groups = new Map();
  for (const input of values) {
    const value = owned(input), wire = CausalVersion.encode(value);
    bytes += wire.length; if (bytes > maxBytes) invalid();
    const dot = value.dot.author + ':' + value.dot.counter, previous = dots.get(dot);
    if (previous !== undefined && previous !== wire) invalid(); dots.set(dot, wire);
    let group = groups.get(value.address);
    if (group === undefined) { group = []; groups.set(value.address, group); }
    group.push(value); if (group.length > 64) invalid();
    result.push(value);
  }
  return { versions: result, groups };
};
const constructorToken = Symbol('causal Merkle snapshot');
const frame = (wire, maxBytes, tag) => {
  if (typeof wire !== 'string' || wire.length > maxBytes) invalid();
  let depth = 0, separators = 0, quoted = false;
  for (const item of wire) {
    if (item < ' ' || item > '~' || item === '\\') invalid();
    if (item === '"') { quoted = !quoted; continue; } if (quoted) continue;
    if (item === '{' || item === '}') invalid();
    if (item === '[') { if (++depth > 5) invalid(); }
    else if (item === ']') { if (--depth < 0) invalid(); }
    else if (item === ',' && ++separators > 32768) invalid();
  }
  if (quoted || depth !== 0) invalid();
  const value = JSON.parse(wire);
  if (!Array.isArray(value) || value.length !== 5 || value[0] !== tag || JSON.stringify(value) !== wire) invalid();
  return value;
};
const nodeCopy = response => {
  object(response);
  if (!Array.isArray(response.children) || response.children.length !== 16) invalid();
  const path = prefix(response.prefix);
  return { root: hash(response.root), prefix: path, children: Array.from(response.children, hash), proof: proof(response.proof, path.length) };
};
const leafCopy = response => {
  object(response); address(response.address);
  return { root: hash(response.root), address: response.address, versions: admission(response.versions, 64, 1048576).versions, proof: proof(response.proof, 16) };
};

export class CausalMerkleSnapshot {
  #rows; #nodes; #empty; #root;
  constructor(token, rows, nodes, empty) {
    if (token !== constructorToken) invalid();
    this.#rows = rows; this.#nodes = nodes; this.#empty = empty; this.#root = nodes.get('') ?? empty[0];
  }
  static async create(values, options = {}) {
    object(options);
    const maxVersions = integer(options.maxVersions ?? 4096, 4096), maxBytes = integer(options.maxBytes ?? 8388608, 8388608);
    // Own all data before the first asynchronous operation.
    const { groups } = admission(values, maxVersions, maxBytes), rows = new Map(), nodes = new Map();
    for (const [key, group] of groups) rows.set(address(key), CausalVersion.merge(group, []));
    const empty = await empties();
    for (const [path, versions] of rows) nodes.set(path, await leafHash(BigInt('0x' + path), versions, empty));
    for (let depth = 15; depth >= 0; depth--) {
      const parents = new Set([...nodes.keys()].filter(path => path.length === depth + 1).map(path => path.slice(0, -1)));
      for (const parent of parents) {
        const children = Array.from({ length: 16 }, (_, digit) => nodes.get(parent + digit.toString(16)) ?? empty[depth + 1]);
        nodes.set(parent, await branch(depth, children));
      }
    }
    return new CausalMerkleSnapshot(constructorToken, rows, nodes, empty);
  }
  get root() { return this.#root; }
  #children(path) {
    return Array.from({ length: 16 }, (_, digit) => this.#nodes.get(path + digit.toString(16)) ?? this.#empty[path.length + 1]);
  }
  #proof(path) {
    return Array.from(path, (digit, depth) => this.#children(path.slice(0, depth)).filter((_, index) => index !== parseInt(digit, 16)));
  }
  node(path) {
    prefix(path); return { root: this.#root, prefix: path, children: this.#children(path), proof: this.#proof(path) };
  }
  leaf(key) {
    const path = address(key);
    return { root: this.#root, address: key, versions: (this.#rows.get(path) ?? []).map(owned), proof: this.#proof(path) };
  }
  static async verifyNode(expectedRoot, response) {
    hash(expectedRoot); object(response);
    if (response.root !== expectedRoot) invalid(); const path = prefix(response.prefix);
    if (!Array.isArray(response.children) || response.children.length !== 16) invalid();
    const children = Array.from(response.children, hash), siblings = proof(response.proof, path.length);
    if (await fold(await branch(path.length, children), path, siblings) !== expectedRoot) invalid();
    return children;
  }
  static async verifyLeaf(expectedRoot, response) {
    hash(expectedRoot); object(response);
    if (response.root !== expectedRoot) invalid(); const path = address(response.address);
    const { versions } = admission(response.versions, 64, 1048576), siblings = proof(response.proof, 16);
    if (versions.some(value => value.address !== response.address)) invalid();
    const merged = CausalVersion.merge(versions, []);
    if (JSON.stringify(merged.map(CausalVersion.encode)) !== JSON.stringify(versions.map(CausalVersion.encode))) invalid();
    const current = await leafHash(BigInt(response.address), versions, await empties());
    if (await fold(current, path, siblings) !== expectedRoot) invalid(); return versions;
  }
  static async encodeNode(response) {
    const value = nodeCopy(response); await CausalMerkleSnapshot.verifyNode(value.root, value);
    return JSON.stringify(['causal-node1', value.root, value.prefix, value.children, value.proof]);
  }
  static async decodeNode(expectedRoot, wire) {
    const value = frame(wire, 32768, 'causal-node1');
    const response = nodeCopy({ root: value[1], prefix: value[2], children: value[3], proof: value[4] });
    await CausalMerkleSnapshot.verifyNode(expectedRoot, response); return response;
  }
  static async encodeLeaf(response) {
    const value = leafCopy(response); await CausalMerkleSnapshot.verifyLeaf(value.root, value);
    return JSON.stringify(['causal-leaf1', value.root, value.address,
      value.versions.map(version => JSON.parse(CausalVersion.encode(version))), value.proof]);
  }
  static async decodeLeaf(expectedRoot, wire) {
    const value = frame(wire, 1081344, 'causal-leaf1');
    if (!Array.isArray(value[3]) || value[3].length > 64) invalid();
    const response = leafCopy({ root: value[1], address: value[2], versions:
      value[3].map(version => CausalVersion.decode(JSON.stringify(version))), proof: value[4] });
    await CausalMerkleSnapshot.verifyLeaf(expectedRoot, response); return response;
  }
}

const cursorToken = Symbol('causal pull continuation');
/** Opaque owned frontier, scoped to one source root and monotone destination. */
export class CausalPullCursor {
  #root; #author; #pending;
  constructor(token, root, author, pending) {
    if (token !== cursorToken) invalid();
    this.#root = root; this.#author = author; this.#pending = pending.slice();
    Object.freeze(this);
  }
  get root() { return this.#root; }
  resume(root, author) {
    if (root !== this.#root || author !== this.#author) invalid();
    return this.#pending.slice();
  }
}
Object.freeze(CausalPullCursor.prototype);

/** One finite pull attempt. Resume its frontier on the same monotone owner.
 * On an error, retry from fresh snapshots; accepted leaves remain durable.
 * The caller bounds raw transport frames before parsing. */
export async function pullCausal(source, destination, options = {}) {
  object(source); object(destination); object(options);
  const root = hash(source.root);
  if (typeof source.node !== 'function' || typeof source.leaf !== 'function' ||
      typeof destination.snapshot !== 'function' || typeof destination.receive !== 'function') invalid();
  const maxNodes = integer(options.maxNodeRequests ?? 4096, 65536);
  const maxLeaves = integer(options.maxLeafRequests ?? 64, 4096);
  const maxBytes = integer(options.maxVersionBytes ?? 8388608, 8388608);
  const cursor = options.continuation ?? null;
  if (cursor !== null && (!(cursor instanceof CausalPullCursor) || cursor.root !== root)) invalid();
  const state = await destination.snapshot(), local = await CausalMerkleSnapshot.create(state.versions);
  const pending = cursor === null ? [''] : cursor.resume(root, state.author);
  const progress = { root, nodeRequests: 0, leafRequests: 0, versionBytes: 0, changedLeaves: 0, complete: false, continuation: null };
  const empty = await empties();
  if (root === local.root || root === empty[0]) pending.length = 0;
  const paused = path => {
    pending.push(path);
    progress.continuation = new CausalPullCursor(cursorToken, root, state.author, pending);
    return progress;
  };
  while (pending.length > 0) {
    const path = pending.pop();
    if (path.length === 16) {
      if (progress.leafRequests === maxLeaves) return paused(path);
      progress.leafRequests++;
      const key = BigInt('0x' + path).toString(), response = await source.leaf(key);
      object(response); if (response.address !== key) invalid();
      const versions = await CausalMerkleSnapshot.verifyLeaf(root, response);
      const bytes = versions.reduce((total, value) => total + CausalVersion.encode(value).length, 0);
      if (progress.versionBytes + bytes > maxBytes) return paused(path);
      progress.versionBytes += bytes;
      const changed = await destination.receive(versions);
      if (typeof changed !== 'boolean') invalid(); if (changed) progress.changedLeaves++;
    } else {
      if (progress.nodeRequests === maxNodes) return paused(path);
      progress.nodeRequests++;
      const response = await source.node(path);
      object(response); if (response.prefix !== path) invalid();
      const remote = await CausalMerkleSnapshot.verifyNode(root, response), current = local.node(path).children;
      // Stack order makes requests deterministic in ascending address order.
      for (let digit = 15; digit >= 0; digit--) {
        if (remote[digit] !== current[digit] && remote[digit] !== empty[path.length + 1])
          pending.push(path + digit.toString(16));
      }
    }
  }
  progress.complete = true; return progress;
}
