// What each backend guarantees, stated rather than emulated. A consumer that
// needs a guarantee asks for it with `client.require(...)` and gets an
// UnsupportedError up front instead of a weaker behaviour later.

/** The largest address of the `u32` stores both `clink --serve` ports use. */
export const MAX_U32_ADDRESS = 2n ** 32n - 1n;

const common = {
  protocol: 'links-text/1',
  operations: ['count', 'each', 'create', 'update', 'delete', 'getName', 'setName', 'getByName', 'removeName', 'query', 'exportLinks'],
  // No request carries several writes; a batch is many requests, each applied
  // on its own, so a failure in the middle leaves the earlier ones in place.
  atomicTransactions: false,
  rollback: false,
  // No cursor or limit in the protocol: each/exportLinks return every match.
  pagination: false,
  bulk: 'pipelined-non-atomic',
  // One store per server; nothing shards or merges stores behind the client.
  sharding: false,
};

/** A `clink --serve` server (Rust or C#) reached over TCP. */
export function tcpServerCapabilities(overrides = {}) {
  return Object.freeze({
    ...common,
    backend: 'clink-server',
    transport: 'tcp',
    rawTcp: true,
    addressBits: 32,
    maxAddress: MAX_U32_ADDRESS,
    persistent: true,
    // The Rust server replaces its file after every write without fsync, the
    // C# server writes through a memory-mapped file without flushing: a write
    // that was answered survives the server process, not a power loss. The
    // links and the names live in two files written one after the other, and
    // no write is atomic across both.
    crashConsistency: false,
    durableFlush: false,
    // Creating a link equal to an existing one: the Rust server stores a
    // second link, the C# server (unique doublets) merges it and `create`
    // resolves to null. The protocol does not say which server answers, so
    // this stays null (unknown) unless the caller states it.
    duplicateLinks: null,
    ...overrides,
  });
}

/** The in-memory WebAssembly `Clink` store of `rust/wasm`. */
export function wasmCapabilities(overrides = {}) {
  return Object.freeze({
    ...common,
    backend: 'clink-wasm',
    transport: 'in-process',
    // Browsers have no raw sockets; this backend never pretends to.
    rawTcp: false,
    bulk: 'sequential-non-atomic',
    addressBits: 32,
    maxAddress: MAX_U32_ADDRESS,
    persistent: false,
    crashConsistency: false,
    durableFlush: false,
    // The Rust LinkStorage keeps equal links apart.
    duplicateLinks: true,
    ...overrides,
  });
}

/** Names of the boolean guarantees `require` understands. */
export const GUARANTEES = ['atomicTransactions', 'rollback', 'crashConsistency', 'durableFlush', 'pagination', 'persistent', 'rawTcp', 'sharding'];
