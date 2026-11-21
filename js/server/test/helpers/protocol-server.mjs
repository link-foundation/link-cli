// Helpers of the links server tests over real sockets: clients whose
// notifications can be awaited, collected trace lines and polling.
import assert from 'node:assert/strict';
import { connect } from '../../src/ipc.js';
import { LinksClient } from '../../src/protocol-client.js';
import { diagnostics } from '../../src/protocol-errors.js';
import { TextLinoProtocol } from '../../src/protocol-protocols.js';

export const PUSH_TIMEOUT = 10_000;

/** A client of `address` (any endpoint) whose notifications can be awaited. */
export async function clientOf(address, protocol = new TextLinoProtocol()) {
  return new LinksClient(await connect(address), protocol);
}

/** Runs `action` with the `diagnostics` lines of standard error collected in `lines`. */
export async function traced(action) {
  const lines = [];
  const write = diagnostics.write;
  diagnostics.write = (stream, text) => {
    if (stream === 'stderr') lines.push(...text.split('\n').filter((line) => line !== ''));
    else write(stream, text);
  };
  try {
    await action(lines);
  } finally {
    diagnostics.write = write;
  }
  return lines;
}

export const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** Waits until `ready()` holds, for at most ten seconds. */
export async function until(ready) {
  for (let waited = 0; !ready(); waited += 5) {
    if (waited > PUSH_TIMEOUT) assert.fail('timed out');
    await pause(5);
  }
}
