// IPC endpoint strings, `[kind:]address[?name=value&...]`: a port of
// rust/src/protocol/ipc/endpoint.rs, the registry of custom kinds of
// rust/src/protocol/ipc/mod.rs (`register`, `registered`) and `command_line`
// of rust/src/protocol/ipc/stdio.rs. Adapted from
// js/client/src/ipc-endpoint.js; the errors are Rust's `io::Error`s.
import { IoError } from './protocol-errors.js';
import { parseStrings } from './ipc-json.js';

/** Every built-in endpoint kind, in documentation order (`KINDS`). */
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

const invalid = (message) => new IoError('InvalidInput', message);

const COMMON = ['timeout'];
const RENDEZVOUS = ['poll', 'accept-poll'];
const RING = ['capacity'];
const DATAGRAM = ['chunk', 'rto'];

/**
 * `options_of`: the options a built-in kind accepts on `platform`, in the
 * order the errors list them; `pipe:` has rendezvous options only on Unix.
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
    default:
      // sysvmq, mailslot, copydata and alpc; Endpoint.parse asks only for built-ins.
      return [...COMMON, ...RENDEZVOUS, ...DATAGRAM];
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

/** `str::to_ascii_lowercase`. */
export const asciiLower = (text) => text.replace(/[A-Z]/g, (letter) => letter.toLowerCase());

const registry = new Map();

/** The `{connect, listen}` registered for `kind`, if any (`registered`). */
export const registered = (kind) => registry.get(asciiLower(String(kind)));

const isKind = (prefix) => {
  const lower = asciiLower(prefix);
  return KINDS.includes(lower) || registry.has(lower);
};

/**
 * `register(kind, connect, listen)`: adds a custom kind, or replaces an
 * earlier registration. `connect(endpoint)` resolves to an `IpcStream` and
 * `listen(endpoint)` to a listener; either may be missing.
 */
export function register(kind, connect, listen) {
  const lower = asciiLower(String(kind));
  if (!/^[a-z0-9-]+$/.test(lower)) throw invalid(`invalid endpoint kind \`${lower}\``);
  if (KINDS.includes(lower)) {
    throw new IoError('AlreadyExists', `\`${lower}\` is a built-in endpoint kind`);
  }
  registry.set(lower, { connect: connect ?? undefined, listen: listen ?? undefined });
}

/** Forgets every registered kind (tests). */
export const unregisterAll = () => registry.clear();

const U64_MAX = (1n << 64n) - 1n;

// `str::parse::<u64>`: an optional `+`, then at least one ASCII digit.
const parseU64 = (text) => {
  if (!/^\+?[0-9]+$/.test(text)) return undefined;
  const value = BigInt(text.startsWith('+') ? text.slice(1) : text);
  return value <= U64_MAX ? value : undefined;
};

const byteOrder = (a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b));

/** A parsed endpoint: a kind, an address and options sorted by name. */
export class Endpoint {
  #options;

  constructor(kind, address, options = []) {
    this.kind = kind;
    this.address = address;
    this.#options = new Map([...options].sort(([a], [b]) => byteOrder(a, b)));
    this.options = Object.freeze(Object.fromEntries(this.#options));
    Object.freeze(this);
  }

  /**
   * `Endpoint::parse`: text without a known kind prefix is a TCP
   * `host:port`. An `Endpoint` passes through unchanged.
   */
  static parse(input, platform = process.platform) {
    if (input instanceof Endpoint) return input;
    const text = trimRust(String(input));
    if (text === '') throw invalid('an endpoint must not be empty');
    const colon = text.indexOf(':');
    let kind = 'tcp';
    let rest = text;
    if (colon >= 0 && isKind(text.slice(0, colon))) {
      kind = asciiLower(text.slice(0, colon));
      rest = text.slice(colon + 1);
    } else if (colon < 0 && isKind(text)) {
      kind = asciiLower(text);
      rest = '';
    }
    if (kind === 'exec') {
      if (trimRust(rest) === '') throw invalid('`exec:` needs a command');
      return new Endpoint(kind, rest);
    }
    const question = rest.indexOf('?');
    const address = question < 0 ? rest : rest.slice(0, question);
    const options = new Map();
    if (question >= 0) {
      const builtin = KINDS.includes(kind);
      const allowed = optionsOf(kind, platform);
      for (const pair of rest.slice(question + 1).split('&')) {
        if (pair === '') continue;
        const equals = pair.indexOf('=');
        if (equals < 0) throw invalid(`endpoint option \`${pair}\` needs a \`=value\``);
        const name = asciiLower(pair.slice(0, equals));
        if (builtin && !allowed.includes(name)) {
          throw invalid(
            `\`${kind}:\` endpoints have no option \`${name}\` (options: ${allowed.join(', ')})`,
          );
        }
        options.set(name, pair.slice(equals + 1));
      }
    }
    const endpoint = new Endpoint(kind, address, options);
    endpoint.#validate();
    return endpoint;
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

  /** Rust's `Display`: `kind[:address][?name=value&...]`. */
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
}

/** `command_line`: a JSON array of strings, or words split on white space. */
export function commandLine(address) {
  const text = trimRust(address);
  const argv = text.startsWith('[') ? parseStrings(text) : text === '' ? [] : text.split(SPLIT);
  if (argv.length === 0 || argv[0] === '') throw invalid('`exec:` needs a command');
  return argv;
}
