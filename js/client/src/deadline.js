// AbortSignal and absolute deadlines, shared by every transport.

import { AbortError, DeadlineExceededError } from './errors.js';

/**
 * Resolves `{ signal, deadline, timeout }` into `{ signal, deadline }`, where
 * `deadline` is an absolute time in milliseconds since the epoch (a Date or a
 * number, as `Date.now()` returns) and `timeout` a convenience that becomes
 * `Date.now() + timeout`. The earlier of both wins.
 */
export function scopeOf({ signal, deadline, timeout } = {}) {
  let absolute = deadline instanceof Date ? deadline.getTime() : deadline;
  if (absolute !== undefined && absolute !== null && !Number.isFinite(absolute)) {
    throw new TypeError('deadline must be a Date or a finite number of epoch milliseconds');
  }
  if (timeout !== undefined && timeout !== null) {
    if (!Number.isFinite(timeout) || timeout < 0) throw new TypeError('timeout must be a non-negative number of milliseconds');
    const fromTimeout = Date.now() + timeout;
    absolute = absolute === undefined || absolute === null ? fromTimeout : Math.min(absolute, fromTimeout);
  }
  return { signal: signal ?? undefined, deadline: absolute ?? undefined };
}

/** The error ending a request whose scope is over, or null while it is not. */
export function scopeError({ signal, deadline }, outcome) {
  if (signal?.aborted) {
    return new AbortError(abortMessage(signal), { outcome, cause: signal.reason });
  }
  if (deadline !== undefined && Date.now() >= deadline) {
    return new DeadlineExceededError(`the deadline ${new Date(deadline).toISOString()} passed`, { outcome });
  }
  return null;
}

function abortMessage(signal) {
  const reason = signal.reason;
  if (reason instanceof Error && reason.message) return `the request was aborted: ${reason.message}`;
  return 'the request was aborted';
}

/**
 * Calls `onEnd(errorOf)` once when the scope's signal aborts or its deadline
 * passes, where `errorOf(outcome)` builds the matching error. Returns the
 * function that stops watching.
 */
export function watchScope({ signal, deadline }, onEnd) {
  let timer;
  let done = false;
  const stop = () => {
    done = true;
    if (timer !== undefined) clearTimeout(timer);
    signal?.removeEventListener('abort', aborted);
  };
  const end = (errorOf) => {
    if (done) return;
    stop();
    onEnd(errorOf);
  };
  function aborted() {
    end((outcome) => scopeError({ signal }, outcome));
  }
  if (deadline !== undefined) {
    // setTimeout overflows above 2^31 - 1 ms; re-arm for far deadlines.
    const arm = () => {
      const left = deadline - Date.now();
      if (left <= 0) {
        end((outcome) => new DeadlineExceededError(`the deadline ${new Date(deadline).toISOString()} passed`, { outcome }));
        return;
      }
      timer = setTimeout(arm, Math.min(left, 2 ** 31 - 1));
    };
    arm();
  }
  signal?.addEventListener('abort', aborted, { once: true });
  return stop;
}
