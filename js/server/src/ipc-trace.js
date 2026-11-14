// The `[ipc]` transport log of rust/src/protocol/ipc/mod.rs (`trace!`), on
// standard error when `CLINK_IPC_TRACE=1` or after `setTrace(true)`. Adapted
// from js/client/src/ipc-trace.js; the lines go through `diagnostics.stderr`
// so they interleave with the other diagnostics as in Rust.
import { diagnostics } from './protocol-errors.js';

let enabled;

/** Turns the `[ipc]` log on or off; it starts on when `CLINK_IPC_TRACE=1`. */
export function setTrace(on) {
  enabled = Boolean(on);
}

/** Whether the `[ipc]` log is on (the environment is read once, lazily). */
export function traceEnabled() {
  if (enabled === undefined) enabled = process.env.CLINK_IPC_TRACE === '1';
  return enabled;
}

/** Writes `[ipc] {message}` when the log is on. */
export function trace(message) {
  if (traceEnabled()) diagnostics.stderr(`[ipc] ${message}`);
}

/** Forgets `setTrace`, so the environment is read again (tests). */
export function resetTrace() {
  enabled = undefined;
}
