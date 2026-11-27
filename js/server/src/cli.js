// Command-line argument parsing for the `clink` binary: every flag, alias,
// default and error text of the Rust parser, the help and version texts, and
// the predicates `main` asks the parsed command line. Port of rust/src/cli.rs.
//
// `Cli.parse(args)` takes the arguments after the program name
// (`process.argv.slice(2)`); `Cli.parseFrom(argv)` skips the first element the
// way Rust's `parse_from` does. Both return `{ kind: 'run', cli }`,
// `{ kind: 'help' }` or `{ kind: 'version' }` and throw a plain `Error` carrying
// the exact Rust message on a bad command line.
import { readFileSync } from 'node:fs';
import { ArityRange } from 'links-notation';
import { parseU32 } from './link.js';
import { DEFAULT_IMPORT_DEPTH, isImportDepth, MAX_IMPORT_DEPTH } from './lino-database-input.js';
import { rustTrim } from './query-parser.js';

const DEFAULT_DATABASE_FILENAME = 'db.links';

const U64_MAX = (1n << 64n) - 1n;
const I64_MIN = -(1n << 63n);
const I64_MAX = (1n << 63n) - 1n;
// `ArityRange::validate`: the minimum is shifted 4 bits into a shape word.
const MAX_MIN_ARITY = U64_MAX >> 4n;

// How an option takes its value: `value` a string, `bool` an optional
// boolean literal, `link` a u32 link id, `seq` an i64 sequence point,
// `arity` an `ArityRange`, `depth` a LiNo import limit, `append` one more string of a list, `constant`
// no value: the option sets `value`.
const OPTIONS = [
  { field: 'db', kind: 'value', names: ['-d', '--db', '--data-source', '--data'] },
  { field: 'query', kind: 'value', names: ['-q', '--query', '--apply', '--do'] },
  { field: 'indexedLookups', kind: 'bool', names: ['--indexed-lookups'] },
  { field: 'trace', kind: 'bool', names: ['-t', '--trace'] },
  {
    field: 'autoCreateMissingReferences',
    kind: 'bool',
    names: ['--auto-create-missing-references'],
  },
  { field: 'structure', kind: 'link', names: ['-s', '--structure'] },
  { field: 'before', kind: 'bool', names: ['-b', '--before'] },
  { field: 'changes', kind: 'bool', names: ['-c', '--changes'] },
  { field: 'after', kind: 'bool', names: ['-a', '--after', '--links'] },
  { field: 'linoOutput', kind: 'value', names: ['--out', '--lino-output', '--export'] },
  { field: 'linoInput', kind: 'value', names: ['--in', '--lino-input', '--import'] },
  { field: 'linoInputMaxDepth', kind: 'depth', names: ['--in-max-depth'] },
  {
    field: 'binaryOutput',
    kind: 'value',
    names: ['--export-binary', '--binary-output', '--binary-out'],
  },
  {
    field: 'binaryInput',
    kind: 'value',
    names: ['--import-binary', '--binary-input', '--binary-in'],
  },
  { field: 'always', kind: 'bool', names: ['--always'] },
  { field: 'once', kind: 'bool', names: ['--once'] },
  { field: 'never', kind: 'bool', names: ['--never'] },
  { field: 'triggers', kind: 'bool', names: ['--triggers'] },
  { field: 'triggersFile', kind: 'value', names: ['--triggers-file'] },
  { field: 'embedTriggers', kind: 'bool', names: ['--embed-triggers'] },
  { field: 'transactions', kind: 'bool', names: ['--transactions'] },
  { field: 'transactionsFile', kind: 'value', names: ['--transactions-file'] },
  { field: 'commitMode', kind: 'value', names: ['--commit-mode'] },
  { field: 'retention', kind: 'value', names: ['--retention'] },
  { field: 'vc', kind: 'bool', names: ['--vc'] },
  { field: 'vcFile', kind: 'value', names: ['--vc-file'] },
  { field: 'branch', kind: 'value', names: ['--branch'] },
  { field: 'branchFrom', kind: 'seq', names: ['--branch-from'] },
  { field: 'checkout', kind: 'value', names: ['--checkout'] },
  { field: 'tag', kind: 'value', names: ['--tag'] },
  { field: 'listBranches', kind: 'bool', names: ['--list-branches'] },
  { field: 'listTags', kind: 'bool', names: ['--list-tags'] },
  { field: 'showLog', kind: 'bool', names: ['--log'] },
  // Rust accepts no `--graphql=VALUE` form: that spelling is an unknown option.
  { field: 'graphql', kind: 'bool', names: ['--graphql'], inline: false },
  { field: 'jwtSecret', kind: 'value', names: ['--jwt-secret'] },
  { field: 'graphqlAdminSecret', kind: 'value', names: ['--graphql-admin-secret'] },
  { field: 'deepPackages', kind: 'append', names: ['--deep-package'] },
  { field: 'serve', kind: 'append', names: ['--serve'] },
  { field: 'forward', kind: 'value', names: ['--forward'] },
  { field: 'connect', kind: 'value', names: ['--connect'] },
  { field: 'protocol', kind: 'value', names: ['--protocol'] },
  { field: 'externalReferences', kind: 'bool', names: ['--external-references'] },
  { field: 'arity', kind: 'arity', names: ['--arity'] },
  { field: 'packedWidths', kind: 'bool', names: ['--packed-widths'] },
  { field: 'triplets', kind: 'bool', names: ['--triplets'] },
  { field: 'permissions', kind: 'value', names: ['--permissions'] },
  { field: 'subject', kind: 'value', names: ['--subject'] },
  { field: 'tokens', kind: 'value', names: ['--tokens'] },
  { field: 'grantToken', kind: 'value', names: ['--grant-token'] },
  { field: 'revokeToken', kind: 'value', names: ['--revoke-token'] },
  { field: 'token', kind: 'value', names: ['--token'] },
  { field: 'lock', kind: 'value', names: ['--lock'] },
  // `--no-lock` is `--lock none`; Rust has no `--no-lock=VALUE` form.
  { field: 'lock', kind: 'constant', value: 'none', names: ['--no-lock'], inline: false },
  { field: 'lockTimeout', kind: 'value', names: ['--lock-timeout'] },
  { field: 'lockStatus', kind: 'bool', names: ['--lock-status'] },
];

/** Every spelling of an option, short ones included. */
const BY_NAME = new Map(OPTIONS.flatMap((option) => option.names.map((n) => [n, option])));

/** The long spellings that also take `--name=value`. */
const INLINE = new Map(
  OPTIONS.filter((option) => option.inline !== false).flatMap((option) =>
    option.names.filter((n) => n.startsWith('--')).map((n) => [n, option]),
  ),
);

/** The name Rust reports in the errors of an inline `--name=value`. */
const inlineLabel = (option) => option.names.find((n) => n.startsWith('--'));

const HELP_TEXT = [
  'LiNo CLI Tool for managing links data store\n\n',
  'Usage: clink [OPTIONS] [QUERY]\n\n',
  'Arguments:\n',
  '  [QUERY]  LiNo query for CRUD operation\n\n',
  'Options:\n',
  '  -d, --db <DB>, --data-source <DB>, --data <DB>\n',
  '          Path to the links database file [default: db.links]\n',
  '  -q, --query <QUERY>, --apply <QUERY>, --do <QUERY>\n',
  '          LiNo query for CRUD operation\n',
  '      --indexed-lookups\n',
  '          Use optional lookup-index decorators for local links and names (Rust only)\n',
  '  -t, --trace\n',
  '          Enable trace (verbose output), including each transport event of\n',
  '          --serve, --connect and --forward (as CLINK_IPC_TRACE=1 does)\n',
  '      --auto-create-missing-references\n',
  '          Create missing numeric and named references as self-referential point links\n',
  '  -s, --structure <STRUCTURE>\n',
  '          ID of the link to format its structure\n',
  '  -b, --before\n',
  '          Print the state of the database before applying changes\n',
  '  -c, --changes\n',
  '          Print the changes applied by the query\n',
  '  -a, --after, --links\n',
  '          Print the state of the database after applying changes\n',
  '      --in <IN>, --lino-input <IN>, --import <IN>\n',
  '          Read and import a LiNo file into the database\n',
  '      --in-max-depth <N>\n',
  '          LiNo import nesting limit (1 to 128; default: 64)\n',
  '      --out <OUT>, --lino-output <OUT>, --export <OUT>\n',
  '          Write the complete database as a LiNo file\n',
  '      --import-binary <PATH>, --binary-input <PATH>, --binary-in <PATH>\n',
  '          Read and import a store archive in binary links notation\n',
  '      --export-binary <PATH>, --binary-output <PATH>, --binary-out <PATH>\n',
  '          Write the complete database, names included, as a store archive in\n',
  '          binary links notation\n',
  '      --always\n',
  '          Store the query as an always-on persistent transformation trigger\n',
  '      --once\n',
  '          Store the query as a persistent transformation trigger that deletes\n',
  '          itself after it fires\n',
  '      --never\n',
  '          Remove stored persistent transformation triggers matching the query\n',
  '      --triggers\n',
  '          Enable persistent transformation triggers for this command\n',
  '      --triggers-file <FILE>\n',
  '          Path to the persistent transformation trigger links database\n',
  '          (default: <db>.triggers.links)\n',
  '      --embed-triggers\n',
  '          Store persistent transformation triggers directly in the main links\n',
  '          database\n',
  '      --transactions\n',
  '          Enable the transactions layer (default log path: <db>.transitions.log)\n',
  '      --transactions-file <FILE>\n',
  '          Path to the transitions log store (implies --transactions)\n',
  '      --commit-mode <MODE>\n',
  "          Choose 'sync' or 'async' commits (default: sync, implies --transactions)\n",
  '      --retention <SPEC>\n',
  "          Log retention policy: 'infinite', 'sized:<n>', or 'chunked:<n>:<dir>'\n",
  '          (implies --transactions)\n',
  '      --vc\n',
  '          Enable the version-control decorator (implies --transactions)\n',
  '      --vc-file <FILE>\n',
  '          Path to the version-control branches store\n',
  '          (default: <db>.versioncontrol.links)\n',
  '      --branch <NAME>\n',
  '          Switch to a branch (creating it if --branch-from is also passed).\n',
  '          Implies --vc.\n',
  '      --branch-from <SEQ>\n',
  '          When creating a branch with --branch, fork from this sequence point\n',
  '      --checkout <POINT>\n',
  '          Time-travel to a specific transition sequence or named tag.\n',
  '          Implies --vc.\n',
  '      --tag <NAME[=POINT]>\n',
  '          Create a tag at current head or at a sequence/existing tag.\n',
  "          Uses the first '=' whose complete suffix resolves.\n",
  '          Implies --vc.\n',
  '      --list-branches\n',
  '          List version-control branches and exit\n',
  '      --list-tags\n',
  '          List version-control tags and exit\n',
  '      --log\n',
  '          Print the transitions log and exit (implies --transactions)\n',
  '      --serve <ADDR>\n',
  '          Serve the database on an endpoint until stopped: a TCP address\n',
  '          (e.g. 127.0.0.1:7878) or KIND:ADDRESS for another transport, such as\n',
  '          unix:/tmp/links.sock, pipe:NAME, fifo:DIR, shm:DIR, mmap:DIR, file:DIR,\n',
  '          signal:DIR, mq:DIR, sysvmq:DIR, udp:HOST:PORT, stdio, mailslot:DIR,\n',
  '          copydata:DIR or alpc:DIR (see docs/protocol/ipc-transports-v1.md);\n',
  '          repeat it to serve one store on several endpoints at once. Each\n',
  '          message may use the text or the binary LiNo protocol\n',
  '      --forward <ADDR>\n',
  '          With --serve: relay every connection, byte for byte, to this\n',
  '          endpoint instead of serving a database (stacks transports)\n',
  '      --graphql\n',
  "          With --serve: speak Hasura's GraphQL over HTTP and WebSocket for Deep\n",
  '      --jwt-secret <SECRET>\n',
  '          Signs the tokens of --graphql (default: random for each run)\n',
  '      --graphql-admin-secret <SECRET>\n',
  '          The x-hasura-admin-secret that bypasses --permissions in --graphql\n',
  '      --deep-package <FILE>\n',
  '          Load a Deep package (JSON rows of links) before --graphql serves\n',
  '      --connect <ADDR>\n',
  '          Send the query to a clink server on a --serve endpoint (e.g.\n',
  '          127.0.0.1:7878 or shm:/tmp/links) instead of opening a database;\n',
  '          an empty query reads every link\n',
  '      --protocol <PROTOCOL>\n',
  "          'text' or 'binary' for --connect (default: text); 'text', 'binary'\n",
  "          or 'any' for --serve (default: any)\n",
  '      --external-references\n',
  '          Binary protocol: send numbers and characters as external references\n',
  '      --arity <RANGE>\n',
  "          Binary protocol: link lengths to use, 'n', 'min..max' or 'min..'\n",
  '          (default: 2, doublets only; e.g. 2..3 adds triplets, 1.. any length)\n',
  '      --packed-widths\n',
  '          Binary protocol: give each section of links the narrowest reference\n',
  '          width it needs instead of one width for the whole packet\n',
  '      --triplets\n',
  '          With --serve: read every request as triplets, (create: (type from to)),\n',
  '          (each: (index type from to)) and three-part substitutions, over this\n',
  '          store of doublets; without it, every request is about doublets\n',
  '      --permissions <RULES>\n',
  '          Check every read and write against permission rules stored as links:\n',
  "          'self' (rules in the data) or a links file of rules; without it,\n",
  '          nothing is checked\n',
  '      --subject <LINK>\n',
  '          The subject (a link index or name) queries run as with --permissions,\n',
  '          and the subject of --serve connections before (auth: token)\n',
  '          (default: 0, a guest)\n',
  '      --tokens <TOKENS>\n',
  '          The token table of (auth: token) for --serve, --grant-token and\n',
  "          --revoke-token: 'self' (tokens in the data) or a links file\n",
  '      --grant-token <TOKEN>\n',
  '          Let a token authenticate as --subject in the --tokens table, then exit\n',
  '      --revoke-token <TOKEN>\n',
  '          Forget a token of the --tokens table, then exit\n',
  '      --token <TOKEN>\n',
  '          With --connect: authenticate with (auth: token) before sending the query\n',
  '      --lock <MODE>\n',
  "          How concurrent invocations on one store are serialized: 'flock'\n",
  '          (default; an OS lock on <db>.lock, released when the holder exits),\n',
  "          'pid' (the directory <db>.lock.d, broken once its owner process is\n",
  "          gone) or 'none'. Writers lock exclusively, read-only runs share it\n",
  '      --no-lock\n',
  '          Same as --lock none, for callers that serialize access themselves\n',
  '      --lock-timeout <MS>\n',
  '          How long to wait for a locked store before failing with the name of\n',
  '          its owner (default: 30000; 0 fails at once)\n',
  '      --lock-status\n',
  '          Print who holds the locks of the store, then exit\n',
  '  -h, --help\n',
  '          Print help\n',
  '  -V, --version\n',
  '          Print version\n',
].join('');

/** Rust's `bool_literal`: the ASCII-case-insensitive boolean spellings. */
function boolLiteral(value) {
  const lower = value.replace(/[A-Z]/g, (letter) => letter.toLowerCase());
  if (['true', '1', 'yes', 'on'].includes(lower)) return true;
  if (['false', '0', 'no', 'off'].includes(lower)) return false;
  return null;
}

function parseBool(option, value) {
  const parsed = boolLiteral(value);
  if (parsed === null) throw new Error(`invalid boolean value '${value}' for ${option}`);
  return parsed;
}

function parseLinkId(option, value) {
  const parsed = parseU32(value);
  if (parsed === null) throw new Error(`invalid link id '${value}' for ${option}`);
  return parsed;
}

function parseImportDepth(value) {
  const depth = parseU32(value);
  if (!isImportDepth(depth)) {
    throw new Error(
      `invalid value for '--in-max-depth': expected an integer from 1 to ${MAX_IMPORT_DEPTH}`,
    );
  }
  return depth;
}

/** Rust's `str::parse::<i64>()`, kept exact as a BigInt. */
function parseSeq(option, value) {
  if (/^[+-]?[0-9]+$/.test(value)) {
    const parsed = BigInt(value);
    if (parsed >= I64_MIN && parsed <= I64_MAX) return parsed;
  }
  throw new Error(`invalid sequence value '${value}' for ${option}`);
}

const arityError = (message) => new Error(`invalid value for '--arity': ${message}`);

/** `ArityRange::from_str` plus `validate`, with the Rust error texts, as clap reports them. */
function parseArity(value) {
  const text = rustTrim(value);
  const number = (part) => {
    if (/^\+?[0-9]+$/.test(part)) {
      const parsed = BigInt(part);
      if (parsed <= U64_MAX) return parsed;
    }
    throw arityError(`invalid arity '${text}': expected n, min..max or min..`);
  };
  const split = text.indexOf('..');
  let min;
  let max;
  if (split < 0) {
    min = number(text);
    max = min;
  } else {
    min = number(text.slice(0, split));
    const rest = text.slice(split + 2);
    max = rest === '' ? null : number(rest);
  }
  if (min === 0n) throw arityError('arity must be at least 1');
  if (min > MAX_MIN_ARITY) throw arityError(`arity ${min} is too large`);
  if (max !== null && max < min) throw arityError(`arity range ${min}..${max} is empty`);
  return new ArityRange(min, max);
}

/** The parsed `clink` command line; field names are the Rust ones in camelCase. */
export class Cli {
  constructor() {
    this.db = DEFAULT_DATABASE_FILENAME;
    this.query = null;
    this.queryArg = null;
    this.trace = false;
    this.indexedLookups = false;
    this.autoCreateMissingReferences = false;
    this.structure = null;
    this.before = false;
    this.changes = false;
    this.after = false;
    this.linoInput = null;
    this.linoInputMaxDepth = DEFAULT_IMPORT_DEPTH;
    this.linoOutput = null;
    this.binaryInput = null;
    this.binaryOutput = null;
    this.always = false;
    this.once = false;
    this.never = false;
    this.triggers = false;
    this.triggersFile = null;
    this.embedTriggers = false;
    this.transactions = false;
    this.transactionsFile = null;
    this.commitMode = null;
    this.retention = null;
    this.vc = false;
    this.vcFile = null;
    this.branch = null;
    /** An i64 sequence point as a BigInt, or null. */
    this.branchFrom = null;
    this.checkout = null;
    this.tag = null;
    this.listBranches = false;
    this.listTags = false;
    this.showLog = false;
    /** Every `--serve` endpoint, in the order given (Rust's `serve` then `serve_also`). */
    this.serve = [];
    this.forward = null;
    this.graphql = false;
    this.jwtSecret = null;
    this.graphqlAdminSecret = null;
    this.deepPackages = [];
    this.connect = null;
    this.protocol = null;
    this.externalReferences = false;
    this.arity = new ArityRange(2n, 2n);
    this.packedWidths = false;
    this.triplets = false;
    this.permissions = null;
    this.subject = null;
    this.tokens = null;
    this.grantToken = null;
    this.revokeToken = null;
    this.token = null;
    /** `--lock` (`--no-lock` is `none`), `--lock-timeout` and `--lock-status`; see store-lock.js. */
    this.lock = null;
    this.lockTimeout = null;
    this.lockStatus = false;
  }

  /** Parses the arguments after the program name. */
  static parse(args) {
    const cli = new Cli();
    let position = 0;
    const nextValue = (option) => {
      if (position >= args.length) throw new Error(`missing value for option '${option}'`);
      return args[position++];
    };
    const nextBool = () => {
      const value = position < args.length ? boolLiteral(args[position]) : null;
      if (value === null) return true;
      position += 1;
      return value;
    };
    while (position < args.length) {
      const arg = args[position++];
      const equals = arg.startsWith('--') ? arg.indexOf('=') : -1;
      const inline = equals > 0 ? INLINE.get(arg.slice(0, equals)) : undefined;
      if (inline !== undefined) {
        cli.#set(inline, inlineLabel(inline), arg.slice(equals + 1));
        continue;
      }
      if (arg === '-h' || arg === '--help') return { kind: 'help' };
      if (arg === '-V' || arg === '--version') return { kind: 'version' };
      const option = BY_NAME.get(arg);
      if (option !== undefined) {
        if (option.kind === 'bool') cli[option.field] = nextBool();
        else if (option.kind === 'constant') cli[option.field] = option.value;
        else cli.#set(option, arg, nextValue(arg));
        continue;
      }
      if (arg === '--') {
        while (position < args.length) cli.#setPositionalQuery(args[position++]);
        break;
      }
      if (arg.startsWith('-')) throw new Error(`unknown option '${arg}'`);
      cli.#setPositionalQuery(arg);
    }
    return { kind: 'run', cli };
  }

  /** Rust's `parse_from`: the first element is the program name and is skipped. */
  static parseFrom(argv) {
    return Cli.parse(argv.slice(1));
  }

  static helpText() {
    return HELP_TEXT;
  }

  /** `clink {version}`, the package version (printed with a trailing newline). */
  static versionText() {
    const manifest = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8'));
    return `clink ${manifest.version}`;
  }

  /** Stores `value` of `option`, reporting a bad value under `label`. */
  #set(option, label, value) {
    switch (option.kind) {
      case 'bool':
        this[option.field] = parseBool(label, value);
        break;
      case 'link':
        this[option.field] = parseLinkId(label, value);
        break;
      case 'depth':
        this[option.field] = parseImportDepth(value);
        break;
      case 'seq':
        this[option.field] = parseSeq(label, value);
        break;
      case 'arity':
        this[option.field] = parseArity(value);
        break;
      case 'append':
        this[option.field].push(value);
        break;
      default:
        this[option.field] = value;
    }
  }

  #setPositionalQuery(value) {
    if (this.queryArg !== null) {
      throw new Error(`unexpected extra positional argument '${value}'`);
    }
    this.queryArg = value;
  }

  /** The `--serve` endpoints after the first (Rust's `serve_also`). */
  get serveAlso() {
    return this.serve.slice(1);
  }

  /** Every `--serve` endpoint, in the order given. */
  serveEndpoints() {
    return [...this.serve];
  }

  /** True when any flag in the transactions decorator family was passed. */
  transactionsRequested() {
    return (
      this.transactions ||
      this.transactionsFile !== null ||
      this.commitMode !== null ||
      this.retention !== null ||
      this.showLog ||
      this.vcRequested()
    );
  }

  /** How many of the trigger commands `--always`, `--once`, `--never` were passed. */
  triggerCommandCount() {
    return Number(this.always) + Number(this.once) + Number(this.never);
  }

  /**
   * True when any flag in the persistent transformation family was passed
   * (the "triggers file already exists" clause lives in `main`).
   */
  persistentTransformationsRequested() {
    return (
      this.always ||
      this.once ||
      this.never ||
      this.triggers ||
      this.embedTriggers ||
      this.triggersFile !== null
    );
  }

  /** True when any flag in the version-control decorator family was passed. */
  vcRequested() {
    return (
      this.vc ||
      this.vcFile !== null ||
      this.branch !== null ||
      this.branchFrom !== null ||
      this.checkout !== null ||
      this.tag !== null ||
      this.listBranches ||
      this.listTags
    );
  }
}
