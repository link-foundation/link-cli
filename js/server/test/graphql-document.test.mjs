// The GraphQL documents of requests: how rust/src/graphql/document.rs reads
// them and what it says of those it cannot read, recorded from the Rust
// `Document::parse` of each text.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';

import { Document } from '../src/graphql-document.js';

describe('Document.parse', () => {
  it('reads or refuses each text as the Rust parser does', () => {
    // `null` where the text is a document.
    for (const [text, error] of [
      ['{a(x:1.5 y:1.5.5)}', "Invalid number '1.5.5'."],
      ['query { a } b', "Unknown operation type 'b'."],
      ['{a(x: )}', "Expected a value, found Some(Punctuator(')'))."],
      ['{a(x: 2e3) b(c: 3)', 'Expected a name, found None.'],
      ['{a(x: abc) b(c: "x" }', "Expected a name, found Some(Punctuator('}'))."],
      ['query 5', "Expected '{', found Some(Int(5))."],
      ['fragment F { a }', "Expected 'on' in fragment F."],
      ['{a(x: """x""") (}', "Expected a name, found Some(Punctuator('('))."],
      ['{a(x: -) }', "Invalid number '-'."],
      ['query 1.5', "Expected '{', found Some(Float(1.5))."],
      ['query 1e16', "Expected '{', found Some(Float(1e16))."],
      ['query 1e-7', "Expected '{', found Some(Float(1e-7))."],
      ['query -0.0', "Expected '{', found Some(Float(-0.0))."],
      ['query "s\\u0301\\n"', 'Expected \'{\', found Some(String("s\\u{301}\\n")).'],
      ['query """b"""', 'Expected \'{\', found Some(String("b")).'],
      ['query Q R {a}', 'Expected \'{\', found Some(Name("R")).'],
      ['query Q @ {a}', "Expected a name, found Some(Punctuator('{'))."],
      ['{a(x: 1 y) }', "Expected ':', found Some(Punctuator(')'))."],
      ['{a: 1}', 'Expected a name, found Some(Int(1)).'],
      ['{... on 5}', 'Expected a name, found Some(Int(5)).'],
      ['query é', "Unexpected character 'é' in the document."],
      ['{a(x: "unterminated)}', 'Unterminated string.'],
      ['{a(x: """x"") }', 'Unterminated block string.'],
      ['{a(x: 1..2)}', "Invalid number '1..2'."],
      ['{a(x: .5)}', "Unexpected '.' in the document."],
      ['query "a\\"\\t"', 'Expected \'{\', found Some(String("a\\"\\t")).'],
      ['{a #c\n b}', null],
      ['{a(x: 123456789012345678901) }', null],
      ['{a(x: [1 2]) {b @skip(if: true)}}', null],
    ]) {
      if (error === null) assert.doesNotThrow(() => Document.parse(text), text);
      else assert.throws(() => Document.parse(text), { message: error }, text);
    }
  });
});
