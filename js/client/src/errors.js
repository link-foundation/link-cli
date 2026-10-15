// Errors of the links client. Every error that ends a request carries an
// `outcome` telling what is known about the request it ended:
//
// - `'not-sent'`: nothing reached the backend; retrying cannot duplicate it.
// - `'failed'`: the backend answered and refused it (`(error: ...)`).
// - `'unknown'`: the request may or may not have run (the connection broke,
//   or it was aborted while the backend was working on it). Read back before
//   retrying a write.

/** Base class of every error this package throws. */
export class LinksError extends Error {
  constructor(message, { outcome = 'unknown', cause } = {}) {
    super(message, cause === undefined ? undefined : { cause });
    this.name = new.target.name;
    this.outcome = outcome;
  }
}

/** The backend answered `(error: 'message')`. */
export class RemoteError extends LinksError {
  constructor(message) {
    super(message, { outcome: 'failed' });
    this.remoteMessage = message;
  }
}

/** A message or reply that is not valid LiNo or not the expected shape. */
export class MalformedMessageError extends LinksError {}

/** The peer does not speak the links protocol this client expects. */
export class ProtocolMismatchError extends LinksError {}

/** A request or reply exceeded a configured limit. */
export class LimitExceededError extends LinksError {}

/** An address outside of what the backend can store. */
export class AddressRangeError extends LinksError {
  constructor(message) {
    super(message, { outcome: 'not-sent' });
  }
}

/** The connection is closed or broke. */
export class ConnectionClosedError extends LinksError {}

/** The request was aborted through its AbortSignal. */
export class AbortError extends LinksError {
  constructor(message = 'the request was aborted', options) {
    super(message, options);
  }
}

/** The request's absolute deadline passed. */
export class DeadlineExceededError extends LinksError {
  constructor(message = 'the request deadline passed', options) {
    super(message, options);
  }
}

/** The backend does not offer what was asked for; nothing is emulated. */
export class UnsupportedError extends LinksError {
  constructor(message) {
    super(message, { outcome: 'not-sent' });
  }
}
