// IPC endpoint strings, `[kind:]address[?name=value&...]`, parsed exactly like
// the Rust reference (rust/src/protocol/ipc/endpoint.rs), plus the registry of
// custom kinds.

/** Every built-in endpoint kind, in documentation order. */
export const KINDS = Object.freeze([
  'tcp',
  'udp',
  'unix',
  'pipe',
  'fifo',
  'stdio',
  'exec',
  'shm',
  'mmap',
  'file',
  'signal',
  'mq',
  'sysvmq',
  'mailslot',
  'copydata',
  'alpc',
]);

/** An IPC failure carrying a Node-style `code` such as `ECONNREFUSED`. */
export class IpcError extends Error {
  constructor(message, code = 'EIO', options) {
    super(message, options);
    this.name = 'IpcError';
    this.code = code;
  }
}

export const invalid = (message) => new IpcError(message, 'EINVAL');
export const hungUp = () => new IpcError('the peer hung up', 'ECONNRESET');
export const brokenPipe = () => new IpcError('the peer no longer reads', 'EPIPE');
export const timedOut = () => new IpcError('timed out', 'ETIMEDOUT');
export const outgoingClosed = () => new IpcError('the outgoing direction was closed', 'EPIPE');

const COMMON = ['timeout'];
const RENDEZVOUS = ['poll', 'accept-poll'];
const RING = ['capacity'];
const DATAGRAM = ['chunk', 'rto'];

/**
 * The options a built-in kind accepts on `platform` (default: this one), in
 * the order the Rust errors list them.
 */
export function optionsOf(kind, platform = process.platform) {
  switch (kind) {
    case 'tcp':
    case 'unix':
    case 'stdio':
    case 'exec':
      return [...COMMON];
    case 'pipe':
      return platform === 'win32' ? [...COMMON] : [...COMMON, ...RENDEZVOUS];
    case 'fifo':
      return [...COMMON, ...RENDEZVOUS];
    case 'shm':
    case 'mmap':
    case 'file':
      return [...COMMON, ...RENDEZVOUS, ...RING];
    case 'signal':
      return [...COMMON, ...RENDEZVOUS, ...RING, 'signo'];
    case 'udp':
      return [...COMMON, ...DATAGRAM, 'heartbeat', 'dead'];
    case 'mq':
      return [...COMMON, ...RENDEZVOUS, ...DATAGRAM, 'maxmsg'];
    case 'sysvmq':
    case 'mailslot':
    case 'copydata':
    case 'alpc':
      return [...COMMON, ...RENDEZVOUS, ...DATAGRAM];
    default:
      return [];
  }
}

// Rust's `char::is_whitespace` (the Unicode White_Space property): unlike
// JavaScript's `trim`, it includes U+0085 and excludes U+FEFF.
const WHITE_SPACE =
  '[\\t\\n\\v\\f\\r \\u0085\\u00a0\\u1680\\u2000-\\u200a\\u2028\\u2029\\u202f\\u205f\\u3000]';
const LEADING = new RegExp(`^${WHITE_SPACE}+`, 'u');
const TRAILING = new RegExp(`${WHITE_SPACE}+$`, 'u');
const SPLIT = new RegExp(`${WHITE_SPACE}+`, 'u');

/** Trims like Rust's `str::trim`. */
export const trimRust = (text) => text.replace(LEADING, '').replace(TRAILING, '');

const asciiLower = (text) => text.replace(/[A-Z]/g, (letter) => letter.toLowerCase());

const registry = new Map();

const isKind = (prefix) => {
  const lower = asciiLower(prefix);
  return KINDS.includes(lower) || registry.has(lower);
};

const U64_MAX = (1n << 64n) - 1n;

// `str::parse::<u64>`: an optional `+`, then at least one ASCII digit.
const parseU64 = (text) => {
  if (!/^\+?[0-9]+$/.test(text)) return undefined;
  const value = BigInt(text.startsWith('+') ? text.slice(1) : text);
  return value <= U64_MAX ? value : undefined;
};

const byteOrder = (a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b));

/** A parsed endpoint: a kind, an address and options. */
export class Endpoint {
  #options;

  constructor(kind, address, options) {
    this.kind = kind;
    this.address = address;
    this.#options = new Map([...options].sort(([a], [b]) => byteOrder(a, b)));
    this.options = Object.freeze(Object.fromEntries(this.#options));
    Object.freeze(this);
  }

  /** The raw value of an option, or `undefined`. */
  option(name) {
    return this.#options.get(name);
  }

  /** A numeric option within `min..=max`, or `fallback`. */
  number(name, fallback, min, max) {
    const text = this.#options.get(name);
    if (text === undefined) return fallback;
    const value = parseU64(text);
    if (value === undefined || value < BigInt(min) || value > BigInt(max)) {
      throw invalid(
        `endpoint option \`${name}\` must be a whole number from ${min} to ${max}, found \`${text}\``,
      );
    }
    return Number(value);
  }

  /** A duration option in milliseconds, or `fallback` milliseconds. */
  millis(name, fallback) {
    return this.number(name, fallback, 0, 86_400_000);
  }

  /** The connect and handshake timeout in milliseconds (`timeout`, 10 s by default). */
  timeout() {
    let value;
    try {
      value = this.millis('timeout', 10_000);
    } catch {
      value = 10_000;
    }
    return Math.max(value, 1);
  }

  toString() {
    let text = this.address === '' ? this.kind : `${this.kind}:${this.address}`;
    let separator = '?';
    for (const [name, value] of this.#options) {
      text += `${separator}${name}=${value}`;
      separator = '&';
    }
    return text;
  }

  #validate() {
    if (this.kind !== 'stdio' && this.address === '' && KINDS.includes(this.kind)) {
      throw invalid(`\`${this.kind}:\` endpoints need an address`);
    }
    if (this.kind === 'stdio' && this.address !== '') throw invalid('`stdio` takes no address');
    for (const name of ['timeout', 'poll', 'accept-poll', 'heartbeat', 'dead', 'rto']) {
      this.millis(name, 1);
    }
    this.number('capacity', 65536, 64, 1 << 30);
    this.number('chunk', 4096, 16, 65000);
    this.number('maxmsg', 8, 1, 65536);
    this.number('signo', 12, 1, 64);
  }

  static validated(kind, address, options) {
    const endpoint = new Endpoint(kind, address, options);
    endpoint.#validate();
    return endpoint;
  }
}

/**
 * Parses an endpoint string. Text without a known kind prefix is a TCP
 * `host:port`, so every address accepted before IPC endpoints existed keeps
 * its meaning.
 */
export function parseEndpoint(input) {
  if (input instanceof Endpoint) return input;
  if (typeof input !== 'string') throw new TypeError('an endpoint must be a string');
  const text = trimRust(input);
  if (text === '') throw invalid('an endpoint must not be empty');
  const colon = text.indexOf(':');
  let kind;
  let rest;
  if (colon >= 0 && isKind(text.slice(0, colon))) {
    kind = asciiLower(text.slice(0, colon));
    rest = text.slice(colon + 1);
  } else if (colon < 0 && isKind(text)) {
    kind = asciiLower(text);
    rest = '';
  } else {
    kind = 'tcp';
    rest = text;
  }
  if (kind === 'exec') {
    if (trimRust(rest) === '') throw invalid('`exec:` needs a command');
    return new Endpoint(kind, rest, []);
  }
  const question = rest.indexOf('?');
  const address = question < 0 ? rest : rest.slice(0, question);
  const options = new Map();
  if (question >= 0) {
    const builtin = KINDS.includes(kind);
    const allowed = optionsOf(kind);
    for (const pair of rest.slice(question + 1).split('&')) {
      if (pair === '') continue;
      const equals = pair.indexOf('=');
      if (equals < 0) throw invalid(`endpoint option \`${pair}\` needs a \`=value\``);
      const name = asciiLower(pair.slice(0, equals));
      if (builtin && !allowed.includes(name)) {
        const list = allowed.length === 0 ? '' : ` (options: ${allowed.join(', ')})`;
        throw invalid(`\`${kind}:\` endpoints have no option \`${name}\`${list}`);
      }
      options.set(name, pair.slice(equals + 1));
    }
  }
  return Endpoint.validated(kind, address, options);
}

/** The argv of an `exec:` address: a JSON array of strings, or words split on white space. */
export function commandLine(address) {
  const text = trimRust(address);
  let argv;
  if (text.startsWith('[')) {
    let parsed;
    try {
      parsed = JSON.parse(text);
    } catch (cause) {
      throw new IpcError(`\`exec:\` needs a JSON array of strings: ${cause.message}`, 'EINVAL');
    }
    if (!Array.isArray(parsed) || parsed.some((item) => typeof item !== 'string')) {
      throw invalid('`exec:` needs a JSON array of strings');
    }
    argv = parsed;
  } else {
    argv = text === '' ? [] : text.split(SPLIT);
  }
  if (argv.length === 0 || argv[0] === '') throw invalid('`exec:` needs a command');
  return argv;
}

/**
 * Adds a custom endpoint kind. `connect(endpoint, options)` returns a duplex
 * stream or a promise of one. Built-in kinds cannot be replaced; registering
 * a kind again replaces the earlier one.
 */
export function registerTransport(kind, connect) {
  if (typeof kind !== 'string') throw new TypeError('an endpoint kind must be a string');
  const lower = asciiLower(kind);
  if (!/^[a-z0-9-]+$/.test(lower)) {
    throw invalid(`invalid endpoint kind \`${lower}\``);
  }
  if (KINDS.includes(lower)) {
    throw new IpcError(`\`${lower}\` is a built-in endpoint kind`, 'EEXIST');
  }
  if (typeof connect !== 'function') throw new TypeError('connect must be a function');
  registry.set(lower, connect);
}

/** The connect function registered for `kind`, if any. */
export const registeredConnect = (kind) => registry.get(asciiLower(String(kind)));
