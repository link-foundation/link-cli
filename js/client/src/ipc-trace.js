// The `[ipc]` transport log on standard error, on when the environment
// variable `CLINK_IPC_TRACE` is `1` (as in the Rust reference).

let enabled;

/** Turns the `[ipc]` log on or off; it starts on when `CLINK_IPC_TRACE=1`. */
export function setTrace(on) {
  enabled = Boolean(on);
}

/** Whether the `[ipc]` log is on. */
export function traceEnabled() {
  if (enabled === undefined) enabled = process.env.CLINK_IPC_TRACE === '1';
  return enabled;
}

/** Writes one `[ipc]` line when the log is on. */
export function trace(message) {
  if (traceEnabled()) process.stderr.write(`[ipc] ${message}\n`);
}
