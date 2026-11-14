// The error kinds of rust/src/error.rs (`LinkError`). `message` is the text the
// Rust `Display` implementation prints, so the CLI reports the same lines.

export class LinkError extends Error {
  constructor(message, options) {
    super(message, options);
    this.name = new.target.name;
  }
}

export class NotFoundError extends LinkError {
  constructor(id) {
    super(`Link not found: ${id}`);
    this.id = id;
  }
}

export class ParseError extends LinkError {
  constructor(message, options) {
    super(`Parse error: ${message}`, options);
  }
}

export class QueryError extends LinkError {
  constructor(message, options) {
    super(`Query error: ${message}`, options);
  }
}

export class InvalidFormatError extends LinkError {
  constructor(message, options) {
    super(`Invalid link format: ${message}`, options);
  }
}

export class StorageError extends LinkError {
  constructor(message, options) {
    super(`Storage error: ${message}`, options);
  }
}

export class TransactionError extends LinkError {
  constructor(message, options) {
    super(`Transaction error: ${message}`, options);
  }
}

export class LockError extends LinkError {
  constructor(message, options) {
    super(`Lock error: ${message}`, options);
  }
}

export class AddressOverflowError extends LinkError {
  constructor(address) {
    super(`Address ${address} does not fit into the configured link address type`);
    this.address = address;
  }
}
