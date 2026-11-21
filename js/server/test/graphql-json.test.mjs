// The JSON reader and writer of the GraphQL server, at their edges: the cases
// of rust/tests/graphql_json.rs, and the Rust standard library's texts of
// numbers, strings and UTF-8 errors the port reproduces.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';

import {
  Json,
  debugF64,
  debugStr,
  f64ToI64,
  formatF64,
  parseF64,
  parseI64,
  parseU64,
  trimRust,
  utf8Text,
} from '../src/graphql-json.js';

const refused = (text, message) =>
  assert.throws(
    () => Json.parse(text),
    (error) => error.message.includes(message),
    text,
  );

const roundTrip = (text, written) => assert.equal(Json.parse(text).write(), written, text);

describe('Json.parse', () => {
  it('refuses malformed text', () => {
    for (const [text, message] of [
      ['', 'Invalid JSON at byte 0'],
      ['1 2', 'Unexpected text after JSON at byte 2'],
      ['nul', 'Invalid JSON at byte 0'],
      ['tru', 'Invalid JSON at byte 0'],
      ['fals', 'Invalid JSON at byte 0'],
      ['?', 'Invalid JSON at byte 0'],
      ['[1', 'Invalid JSON array at byte 2'],
      ['[1 2]', 'Invalid JSON array at byte 3'],
      ['{1:2}', 'Invalid JSON object at byte 1'],
      ['{"a" 1}', 'Invalid JSON object at byte 5'],
      ['{"a":1', 'Invalid JSON object at byte 6'],
      ['{"a":1 "b":2}', 'Invalid JSON object at byte 7'],
      ['{"a":1,}', 'Invalid JSON object at byte 7'],
      ['1.2.3', 'Invalid JSON number at byte 0'],
      ['-', 'Invalid JSON number at byte 0'],
      ['"a', 'Unterminated JSON string'],
      ['"a\\', 'Unterminated JSON string'],
      ['"\\q"', 'Invalid JSON escape'],
      ['"\\u12"', 'Invalid JSON escape'],
      ['"\\u12', 'Invalid JSON escape at byte 3.'],
      ['"\\uzzzz"', 'Invalid JSON escape'],
      ['"\\ud83d\\uzzzz"', 'Invalid JSON escape'],
    ]) {
      refused(text, message);
    }
  });

  it('has a limit on nesting', () => {
    assert.doesNotThrow(() => Json.parse(`${'['.repeat(128)}${']'.repeat(128)}`));
    refused(`${'['.repeat(130)}${']'.repeat(130)}`, 'nested too deeply');
    refused(`${'{"a":'.repeat(130)}1${'}'.repeat(130)}`, 'nested too deeply');
  });

  it('keeps values and escapes through a round trip', () => {
    roundTrip(' [ ] ', '[]');
    roundTrip(' { } ', '{}');
    roundTrip('[null,true,false]', '[null,true,false]');
    roundTrip('{"b":1,"a":[2]}', '{"b":1,"a":[2]}');
    roundTrip('1.5', '1.5');
    roundTrip('-2', '-2');
    roundTrip('"\\"\\\\\\/\\n\\r\\t\\b\\f\\u0041"', '"\\"\\\\/\\n\\r\\t\\u0008\\u000cA"');
    roundTrip('"\\u0001"', '"\\u0001"');
    roundTrip('"\\ud83d\\ude00"', '"😀"');
    // A lone surrogate has no character of its own.
    roundTrip('"\\ud83d"', '"�"');
    roundTrip('"\\ude00"', '"�"');
    // Rust's `from_str_radix` takes a sign.
    roundTrip('"\\u+041"', '"A"');
    roundTrip('"é"', '"é"');
  });

  it('reads numbers as what they are', () => {
    const whole = Json.float(3.0);
    assert.equal(whole.asI64(), 3n);
    assert.equal(whole.asF64(), 3);
    assert.equal(Json.float(3.5).asI64(), null);
    assert.equal(Json.float(Infinity).asI64(), null);
    assert.equal(Json.int(3).asF64(), 3);
    assert.equal(Json.NULL.asF64(), null);
    assert.equal(Json.NULL.asI64(), null);
    assert.equal(Json.int(1).asStr(), null);
    assert.equal(Json.int(1).get('a'), null);
    assert.deepEqual(Json.int(1).asList(), [Json.int(1)]);
    assert.deepEqual(Json.NULL.asList(), []);
    assert.equal(Json.float(1.5).write(), '1.5');
    assert.equal(Json.float(NaN).write(), 'null');
    assert.equal(Json.float(Infinity).write(), 'null');
    assert.equal(Json.parse('99999999999999999999').asF64(), 1e20);
    assert.equal(Json.parse('9223372036854775807').asI64(), 2n ** 63n - 1n);
    assert.equal(Json.parse('1e2').kind, 'float');
  });

  it('escapes control characters when written', () => {
    assert.equal(Json.str('\u0000\u001f"\\\n').write(), '"\\u0000\\u001f\\"\\\\\\n"');
    assert.equal(Json.bool(false).write(), 'false');
  });

  it('keeps a present null apart from a missing key', () => {
    const object = Json.parse('{"a":null,"a":1}');
    assert.equal(object.get('a'), Json.NULL);
    assert.equal(object.get('b'), null);
    assert.equal(object.write(), '{"a":null,"a":1}');
  });

  it('compares as the derived PartialEq', () => {
    const equal = (left, right) => Json.parse(left).equals(Json.parse(right));
    assert.ok(equal('[1,{"a":"x"}]', '[1,{"a":"x"}]'));
    assert.ok(!equal('[1]', '[1,2]'));
    assert.ok(!equal('[1]', '[2]'));
    assert.ok(!equal('{"a":1}', '{"b":1}'));
    assert.ok(!equal('{"a":1}', '{"a":2}'));
    assert.ok(!equal('{"a":1}', '{"a":1,"b":2}'));
    assert.ok(!equal('1', '1.0'));
    assert.ok(equal('null', 'null'));
  });
});

describe('the texts of Rust numbers', () => {
  it('parses integers as str::parse does', () => {
    assert.equal(parseI64('+12'), 12n);
    assert.equal(parseI64('-9223372036854775808'), -(2n ** 63n));
    assert.equal(parseI64('9223372036854775808'), null);
    assert.equal(parseI64('1.0'), null);
    assert.equal(parseU64('18446744073709551615'), 2n ** 64n - 1n);
    assert.equal(parseU64('18446744073709551616'), null);
    assert.equal(parseU64('-1'), null);
    assert.equal(parseF64('.5'), 0.5);
    assert.equal(parseF64('5.'), 5);
    assert.equal(parseF64('1e'), null);
  });

  it('casts f64 to i64 as `as` does', () => {
    assert.equal(f64ToI64(NaN), 0n);
    assert.equal(f64ToI64(1e19), 2n ** 63n - 1n);
    assert.equal(f64ToI64(-1e19), -(2n ** 63n));
    assert.equal(f64ToI64(-2.7), -2n);
  });

  it('formats f64 as Display and Debug do', () => {
    // Recorded from rustc's `{}` and `{:?}` of each value.
    for (const [value, display, debug] of [
      [1, '1', '1.0'],
      [1e16, '10000000000000000', '1e16'],
      [1.5e-5, '0.000015', '1.5e-5'],
      [1e-4, '0.0001', '0.0001'],
      [1e15, '1000000000000000', '1000000000000000.0'],
      [-0, '-0', '-0.0'],
      [0, '0', '0.0'],
      [NaN, 'NaN', 'NaN'],
      [Infinity, 'inf', 'inf'],
      [-Infinity, '-inf', '-inf'],
      [0.1, '0.1', '0.1'],
      [-2.5e20, '-250000000000000000000', '-2.5e20'],
      [123.456, '123.456', '123.456'],
      [1e-7, '0.0000001', '1e-7'],
      [9.99e15, '9990000000000000', '9990000000000000.0'],
    ]) {
      assert.equal(formatF64(value), display, `${value}`);
      assert.equal(debugF64(value), debug, `${value}`);
    }
  });
});

describe('the texts of Rust strings', () => {
  it('quotes a str as Debug does', () => {
    // Recorded from rustc's `{:?}` of each string.
    for (const [text, debug] of [
      ['á', '"a\\u{301}"'],
      ['\u007f', '"\\u{7f}"'],
      ['  ', '" \\u{a0}"'],
      ['\0\t\r\n\\"\'', '"\\0\\t\\r\\n\\\\\\"\'"'],
      ['​', '"\\u{200b}"'],
      [' ', '"\\u{2028}"'],
      ['', '"\\u{e000}"'],
      ['͸', '"\\u{378}"'],
      ['é😀', '"é😀"'],
    ]) {
      assert.equal(debugStr(text), debug);
    }
  });

  it('reports bytes that are not UTF-8 as Utf8Error does', () => {
    // Recorded from rustc's `String::from_utf8(bytes).unwrap_err()`.
    for (const [bytes, error] of [
      [[0xff], 'invalid utf-8 sequence of 1 bytes from index 0'],
      [[0xc3], 'incomplete utf-8 byte sequence from index 0'],
      [[0xe0, 0x80], 'invalid utf-8 sequence of 1 bytes from index 0'],
      [[0xe2, 0x82], 'incomplete utf-8 byte sequence from index 0'],
      [[0xf0, 0x9f, 0x98, 0x41], 'invalid utf-8 sequence of 3 bytes from index 0'],
      [[0x41, 0xed, 0xa0, 0x80], 'invalid utf-8 sequence of 1 bytes from index 1'],
      [[0xf4, 0x90, 0x80, 0x80], 'invalid utf-8 sequence of 1 bytes from index 0'],
      [[0xc2, 0x41], 'invalid utf-8 sequence of 1 bytes from index 0'],
      [[0x80], 'invalid utf-8 sequence of 1 bytes from index 0'],
      [[0xe0, 0xa0, 0x80, 0xff], 'invalid utf-8 sequence of 1 bytes from index 3'],
      [
        [0xc3, 0xa9, 0xe2, 0x82, 0xac, 0xf0, 0x9f, 0x98, 0x80, 0xf0, 0x90],
        'incomplete utf-8 byte sequence from index 9',
      ],
    ]) {
      assert.throws(() => utf8Text(Uint8Array.from(bytes)), { message: error });
    }
    assert.equal(utf8Text(Uint8Array.from([0xc3, 0xa9])), 'é');
  });

  it('trims Unicode white space as str::trim does', () => {
    assert.equal(trimRust('　 x\u0085 '), 'x');
  });
});
