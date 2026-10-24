// @link-foundation/link-cli-client: one links contract over interchangeable
// backends. This entry point imports nothing platform-specific, so it loads
// in browsers as well as Node.js, Bun and Deno; the TCP and process adapters
// live in `./tcp` and `./process`.

export { LinksClient, LinksDecorator } from './client.js';
export { createWasmClient, WasmTransport } from './wasm.js';
export { EngineClient, WasmEngine } from './engine.js';
export { MemoryCommitLog, ReplicatedCommitLog } from './engine-logs.js';
export { GUARANTEES, MAX_U32_ADDRESS, tcpServerCapabilities, wasmCapabilities } from './capabilities.js';
export { scopeOf } from './deadline.js';
export {
  AbortError,
  AddressRangeError,
  ConnectionClosedError,
  DeadlineExceededError,
  LimitExceededError,
  LinksError,
  MalformedMessageError,
  ProtocolMismatchError,
  RemoteError,
  UnsupportedError,
} from './errors.js';
export { DEFAULT_MAX_MESSAGE_BYTES, encodeMessage, MessageReader } from './framing.js';
export { formatDocument, formatReference, group, named, parseDocument } from './lino.js';
export { ANY, toAddress } from './operations.js';
