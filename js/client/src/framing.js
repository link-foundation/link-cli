// The text protocol framing of `clink --serve`: a message is its LiNo lines
// followed by a line holding only `.`; a line starting with `.` gets one more
// `.` (SMTP dot-stuffing). Readers accept `\n` and `\r\n`.

import { LimitExceededError, MalformedMessageError } from './errors.js';

/** The server's default limit of one text message: 64 MiB. */
export const DEFAULT_MAX_MESSAGE_BYTES = 64 * 1024 * 1024;

const encoder = new TextEncoder();

/** Frames LiNo text as one message, as UTF-8 bytes. */
export function encodeMessage(text) {
  let out = '';
  if (text !== '') {
    for (const line of text.split('\n')) out += `${line.startsWith('.') ? '.' : ''}${line}\n`;
  }
  return encoder.encode(`${out}.\n`);
}

/**
 * Splits a byte stream into messages. Feed it chunks with `push`; it returns
 * the text of every message they completed. A message longer than `maxBytes`
 * (counted like the server: the unstuffed content plus line breaks) throws
 * {@link LimitExceededError} without buffering the rest of it.
 */
export class MessageReader {
  constructor({ maxBytes = DEFAULT_MAX_MESSAGE_BYTES } = {}) {
    this.maxBytes = maxBytes;
    this.decoder = new TextDecoder('utf-8', { fatal: true });
    this.pending = []; // byte chunks of the current, unfinished line
    this.pendingBytes = 0;
    this.lines = [];
    this.messageBytes = 0;
  }

  /** Whether a message is partially read. */
  get midMessage() {
    return this.pendingBytes > 0 || this.lines.length > 0;
  }

  push(chunk) {
    const messages = [];
    let start = 0;
    for (;;) {
      const newline = chunk.indexOf(10, start);
      const end = newline === -1 ? chunk.length : newline;
      const piece = chunk.subarray(start, end);
      if (this.messageBytes + this.pendingBytes + piece.length > this.maxBytes + 2) {
        throw new LimitExceededError(`message longer than ${this.maxBytes} bytes`);
      }
      if (newline === -1) {
        if (piece.length > 0) {
          this.pending.push(piece);
          this.pendingBytes += piece.length;
        }
        return messages;
      }
      const message = this.endLine(piece);
      if (message !== null) messages.push(message);
      start = newline + 1;
    }
  }

  endLine(last) {
    let bytes = last;
    if (this.pending.length > 0) {
      bytes = new Uint8Array(this.pendingBytes + last.length);
      let offset = 0;
      for (const part of this.pending) {
        bytes.set(part, offset);
        offset += part.length;
      }
      bytes.set(last, offset);
      this.pending = [];
      this.pendingBytes = 0;
    }
    if (bytes[bytes.length - 1] === 13) bytes = bytes.subarray(0, bytes.length - 1);
    if (bytes.length === 1 && bytes[0] === 46) {
      const lines = this.lines;
      this.lines = [];
      this.messageBytes = 0;
      return lines.join('\n');
    }
    if (bytes[0] === 46) bytes = bytes.subarray(1);
    let line;
    try {
      line = this.decoder.decode(bytes);
    } catch {
      throw new MalformedMessageError('text message is not valid UTF-8');
    }
    this.messageBytes += bytes.length + (this.lines.length > 0 ? 1 : 0);
    if (this.messageBytes > this.maxBytes) {
      throw new LimitExceededError(`message longer than ${this.maxBytes} bytes`);
    }
    this.lines.push(line);
    return null;
  }
}
