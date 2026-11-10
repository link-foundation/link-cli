// Bounded waiting for pessimistic sessions. The engine's refusal stays synchronous
// because JavaScript has one thread: a waiter undoes its request, gives up the
// thread, and repeats the request when an owner may have released its address.

import { scopeError, watchScope } from './deadline.js';
import { LinksError } from './errors.js';

// setTimeout overflows above this; the loop continues after each slice.
const LONGEST_SLICE_MS = 2 ** 31 - 1;

/** Shared by the sessions of one engine; exists only when `lockWaitMs` is selected. */
export class LockWaits {
  #limitMs;
  // Waiter to owner. Each session waits for one owner at a time.
  #edges = new Map();
  #wakers = new Set();

  constructor(limitMs) {
    this.#limitMs = limitMs;
  }

  /** A session ended: every waiter checks again, and records its edge if it still waits. */
  wakeAll() {
    this.#edges.clear();
    for (const wake of [...this.#wakers]) wake();
  }

  #closesCycle(waiter, holder) {
    for (let session = holder; session !== undefined; session = this.#edges.get(session)) {
      if (session === waiter) return true;
    }
    return false;
  }

  #wait(waiter, holder, remainingMs, scope) {
    this.#edges.set(waiter, holder);
    return new Promise((resolve, reject) => {
      let stopWatching;
      const end = () => {
        clearTimeout(timer);
        this.#wakers.delete(wake);
        this.#edges.delete(waiter);
      };
      const wake = () => {
        end();
        stopWatching();
        resolve();
      };
      const timer = setTimeout(wake, Math.min(remainingMs, LONGEST_SLICE_MS));
      this.#wakers.add(wake);
      // The scope ended and undone writes make the request as if never sent.
      stopWatching = watchScope(scope, (errorOf) => {
        end();
        reject(errorOf('not-sent'));
      });
    });
  }

  /** Answers like `attempt()`, a request or a lock on `session`, except that a
   * refused attempt runs again. A thrown refusal is thrown once waiting ends. */
  async request(session, attempt, scope) {
    const waiter = session.writeOwner();
    const started = Date.now();
    for (;;) {
      let savepoint;
      try {
        savepoint = session.savepoint();
      } catch {
        // A closed session has nothing to wait for and answers on its own.
        return attempt();
      }
      let reply;
      let failure;
      try {
        reply = attempt();
      } catch (error) {
        failure = { error };
      }
      const answer = () => {
        if (failure) throw failure.error;
        return reply;
      };
      const holder = session.takeWriteBlocker();
      if (holder === undefined) {
        session.releaseSavepoint(savepoint);
        return answer();
      }
      // A request of several writes may have staged some before it was refused.
      session.rollbackTo(savepoint);
      session.releaseSavepoint(savepoint);
      if (this.#closesCycle(waiter, holder)) {
        throw new LinksError(
          'deadlock: waiting for this write address would never end; roll back and retry',
          { outcome: 'failed' },
        );
      }
      const remainingMs = this.#limitMs - (Date.now() - started);
      if (remainingMs <= 0) return answer();
      const late = scopeError(scope, 'not-sent');
      if (late) throw late;
      await this.#wait(waiter, holder, remainingMs, scope);
    }
  }
}
