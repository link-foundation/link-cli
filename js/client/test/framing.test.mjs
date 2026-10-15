import assert from 'node:assert/strict';
import test from 'node:test';
import { encodeMessage, MessageReader } from '../src/framing.js';

const text = (bytes) => new TextDecoder().decode(bytes);
const bytes = (string) => new TextEncoder().encode(string);

test('messages end with a dot line and dot-stuff leading dots', () => {
  assert.equal(text(encodeMessage('(count: (0))')), '(count: (0))\n.\n');
  assert.equal(text(encodeMessage('')), '.\n');
  assert.equal(text(encodeMessage('a\n.b\n.')), 'a\n..b\n..\n.\n');
});

test('the reader splits a stream at any byte, LF or CRLF', () => {
  const stream = bytes('(count: 2)\r\n.\r\n..b\n.\n.\n');
  for (let size = 1; size <= stream.length; size += 1) {
    const reader = new MessageReader();
    const messages = [];
    for (let at = 0; at < stream.length; at += size) messages.push(...reader.push(stream.subarray(at, at + size)));
    assert.deepEqual(messages, ['(count: 2)', '.b', ''], `chunks of ${size}`);
    assert.equal(reader.midMessage, false);
  }
});

test('multi-byte characters split across chunks decode', () => {
  const stream = bytes('(name: 你好)\n.\n');
  const reader = new MessageReader();
  const messages = [];
  for (const byte of stream) messages.push(...reader.push(Uint8Array.of(byte)));
  assert.deepEqual(messages, ['(name: 你好)']);
});

test('a reply over the limit fails without buffering it', () => {
  const reader = new MessageReader({ maxBytes: 8 });
  assert.deepEqual(reader.push(bytes('12345678\n.\n')), ['12345678']);
  assert.throws(() => reader.push(bytes('123456\n12345')), { name: 'LimitExceededError' });
  const endless = new MessageReader({ maxBytes: 8 });
  assert.throws(() => endless.push(bytes('x'.repeat(100))), { name: 'LimitExceededError' });
});

test('invalid UTF-8 is a malformed message', () => {
  assert.throws(() => new MessageReader().push(Uint8Array.of(0xff, 10)), { name: 'MalformedMessageError' });
});
