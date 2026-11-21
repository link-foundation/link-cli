// GraphQL documents: operations, selections, fragments and values.
// Port of rust/src/graphql/document.rs.
//
// A selection is `{ kind: 'field', field }`, `{ kind: 'spread', name,
// directives }` or `{ kind: 'inline', on, directives, selections }`, with
// `on` null for an inline fragment without a type condition. An error names
// the token it found as Rust's `Debug` prints an `Option<&Token>`.

import { Json, debugF64, debugStr, parseF64, parseI64 } from './graphql-json.js';

/** A value of the document: a variable, a literal, a list or an object of values. */
export class Value {
  constructor(kind, value) {
    this.kind = kind;
    this.value = value;
  }

  static variable(name) {
    return new Value('variable', name);
  }

  static literal(json) {
    return new Value('literal', json);
  }

  static list(items) {
    return new Value('list', items);
  }

  static object(pairs) {
    return new Value('object', pairs);
  }

  /** The value with its variables replaced; a variable nobody passed is null. */
  resolve(variables) {
    switch (this.kind) {
      case 'variable':
        return variables.get(this.value) ?? Json.NULL;
      case 'literal':
        return this.value;
      case 'list':
        return Json.array(this.value.map((item) => item.resolve(variables)));
      default:
        return Json.object(this.value.map(([key, value]) => [key, value.resolve(variables)]));
    }
  }
}

export class Field {
  constructor(alias, name, args, directives, selections) {
    this.alias = alias;
    this.name = name;
    this.arguments = args;
    this.directives = directives;
    this.selections = selections;
  }

  responseKey() {
    return this.alias ?? this.name;
  }

  /** The resolved argument, or `null` when it is missing or null. */
  argument(name, variables) {
    const found = this.arguments.find(([key]) => key === name);
    if (!found) return null;
    const value = found[1].resolve(variables);
    return value.isNull() ? null : value;
  }
}

export class Document {
  constructor() {
    this.operations = [];
    /** Fragment name to its selections. */
    this.fragments = new Map();
  }

  static parse(text) {
    const parser = new Parser(tokenize(text));
    const document = new Document();
    while (parser.peek() !== undefined) {
      if (parser.eatName('fragment')) {
        const name = parser.name();
        if (!parser.eatName('on')) throw new Error(`Expected 'on' in fragment ${name}.`);
        parser.name();
        parser.directives(0);
        document.fragments.set(name, parser.selectionSet(0));
      } else {
        document.operations.push(parser.operation());
      }
    }
    if (document.operations.length === 0) {
      throw new Error('A document has at least one operation.');
    }
    return document;
  }

  /** The operation of this name, or the only one when `name` is null. */
  operation(name) {
    if (name !== null) {
      const found = this.operations.find((operation) => operation.name === name);
      if (!found) throw new Error(`Unknown operation named '${name}'.`);
      return found;
    }
    if (this.operations.length === 1) return this.operations[0];
    throw new Error('Must provide operation name if query contains multiple operations.');
  }
}

const PUNCTUATORS = new Set('!$():=@[]{}|&');
const SKIPPED = new Set([' ', '\n', '\r', '\t', ',', '﻿']);
const NUMBER = /^[0-9.eE+-]$/;
const NAME_START = /^[_A-Za-z]$/;
const NAME = /^[_A-Za-z0-9]$/;

const token = (kind, value) => ({ kind, value });

/** Rust's `Debug` of an `Option<&Token>`. */
function debugToken(found) {
  if (found === undefined) return 'None';
  const { kind, value } = found;
  const shown = {
    Punctuator: () => `'${value}'`,
    Name: () => debugStr(value),
    Int: () => value.toString(),
    Float: () => debugF64(value),
    String: () => debugStr(value),
  }[kind]();
  return `Some(${kind}(${shown}))`;
}

function blockString(chars, start) {
  let at = start;
  let out = '';
  for (;;) {
    if (at + 2 >= chars.length) throw new Error('Unterminated block string.');
    if (chars[at] === '"' && chars[at + 1] === '"' && chars[at + 2] === '"') {
      return { at: at + 3, value: out };
    }
    out += chars[at];
    at += 1;
  }
}

function quotedString(chars, start) {
  let at = start + 1;
  while (at < chars.length && chars[at] !== '"') at += chars[at] === '\\' ? 2 : 1;
  if (at >= chars.length) throw new Error('Unterminated string.');
  at += 1;
  // The literal opens with a quote, so it reads as a JSON string or not at all.
  return { at, value: Json.parse(chars.slice(start, at).join('')).value };
}

function number(chars, start) {
  let at = start + 1;
  while (at < chars.length && NUMBER.test(chars[at])) at += 1;
  const literal = chars.slice(start, at).join('');
  const integer = parseI64(literal);
  if (integer !== null) return { at, token: token('Int', integer) };
  const float = parseF64(literal);
  if (float === null) throw new Error(`Invalid number '${literal}'.`);
  return { at, token: token('Float', float) };
}

function tokenize(text) {
  const chars = Array.from(text);
  const tokens = [];
  let at = 0;
  while (at < chars.length) {
    const character = chars[at];
    if (SKIPPED.has(character)) at += 1;
    else if (character === '#') {
      while (at < chars.length && chars[at] !== '\n') at += 1;
    } else if (PUNCTUATORS.has(character)) {
      tokens.push(token('Punctuator', character));
      at += 1;
    } else if (character === '.') {
      if (chars[at + 1] !== '.' || chars[at + 2] !== '.') {
        throw new Error("Unexpected '.' in the document.");
      }
      tokens.push(token('Punctuator', '.'));
      at += 3;
    } else if (character === '"') {
      const read =
        chars[at + 1] === '"' && chars[at + 2] === '"'
          ? blockString(chars, at + 3)
          : quotedString(chars, at);
      tokens.push(token('String', read.value));
      at = read.at;
    } else if (character === '-' || (character >= '0' && character <= '9')) {
      const read = number(chars, at);
      tokens.push(read.token);
      at = read.at;
    } else if (NAME_START.test(character)) {
      const start = at;
      while (at < chars.length && NAME.test(chars[at])) at += 1;
      tokens.push(token('Name', chars.slice(start, at).join('')));
    } else {
      throw new Error(`Unexpected character '${character}' in the document.`);
    }
  }
  return tokens;
}

/** Deep enough for any selection a client means. */
const MAX_DEPTH = 64;

const OPERATION_KINDS = new Map([
  ['query', 'query'],
  ['mutation', 'mutation'],
  ['subscription', 'subscription'],
]);

class Parser {
  constructor(tokens) {
    this.tokens = tokens;
    this.at = 0;
  }

  peek() {
    return this.tokens[this.at];
  }

  isPunctuator(punctuator) {
    const found = this.peek();
    return found !== undefined && found.kind === 'Punctuator' && found.value === punctuator;
  }

  isName() {
    return this.peek()?.kind === 'Name';
  }

  eat(punctuator) {
    if (!this.isPunctuator(punctuator)) return false;
    this.at += 1;
    return true;
  }

  expect(punctuator) {
    if (!this.eat(punctuator)) {
      throw new Error(`Expected '${punctuator}', found ${debugToken(this.peek())}.`);
    }
  }

  eatName(name) {
    if (!this.isName() || this.peek().value !== name) return false;
    this.at += 1;
    return true;
  }

  name() {
    if (!this.isName()) throw new Error(`Expected a name, found ${debugToken(this.peek())}.`);
    const { value } = this.peek();
    this.at += 1;
    return value;
  }

  operation() {
    if (this.isPunctuator('{')) {
      return { kind: 'query', name: null, variables: [], selections: this.selectionSet(0) };
    }
    const keyword = this.name();
    const kind = OPERATION_KINDS.get(keyword);
    if (kind === undefined) throw new Error(`Unknown operation type '${keyword}'.`);
    const name = this.isName() ? this.name() : null;
    /** Variable name and default value, or null without one. */
    const variables = [];
    if (this.eat('(')) {
      while (!this.eat(')')) {
        this.expect('$');
        const variable = this.name();
        this.expect(':');
        this.skipType(0);
        const fallback = this.eat('=') ? this.value(0) : null;
        this.directives(0);
        variables.push([variable, fallback]);
      }
    }
    this.directives(0);
    return { kind, name, variables, selections: this.selectionSet(0) };
  }

  skipType(depth) {
    if (depth > MAX_DEPTH) throw new Error('The type is nested too deeply.');
    if (this.eat('[')) {
      this.skipType(depth + 1);
      this.expect(']');
    } else {
      this.name();
    }
    this.eat('!');
  }

  fragmentSelection(depth) {
    if (this.eatName('on')) {
      const on = this.name();
      const directives = this.directives(depth);
      return { kind: 'inline', on, directives, selections: this.selectionSet(depth + 1) };
    }
    if (this.isName()) {
      const name = this.name();
      return { kind: 'spread', name, directives: this.directives(depth) };
    }
    const directives = this.directives(depth);
    return { kind: 'inline', on: null, directives, selections: this.selectionSet(depth + 1) };
  }

  fieldSelection(depth) {
    const first = this.name();
    const [alias, name] = this.eat(':') ? [first, this.name()] : [null, first];
    const args = this.arguments(depth);
    const directives = this.directives(depth);
    const selections = this.isPunctuator('{') ? this.selectionSet(depth + 1) : [];
    return { kind: 'field', field: new Field(alias, name, args, directives, selections) };
  }

  selectionSet(depth) {
    if (depth > MAX_DEPTH) throw new Error('The selection is nested too deeply.');
    this.expect('{');
    const selections = [];
    while (!this.eat('}')) {
      selections.push(this.eat('.') ? this.fragmentSelection(depth) : this.fieldSelection(depth));
    }
    return selections;
  }

  arguments(depth) {
    const args = [];
    if (this.eat('(')) {
      while (!this.eat(')')) {
        const name = this.name();
        this.expect(':');
        args.push([name, this.value(depth)]);
      }
    }
    return args;
  }

  directives(depth) {
    const directives = [];
    while (this.eat('@')) {
      const name = this.name();
      directives.push({ name, arguments: this.arguments(depth) });
    }
    return directives;
  }

  value(depth) {
    if (depth > MAX_DEPTH) throw new Error('The value is nested too deeply.');
    const found = this.peek();
    this.at += 1;
    switch (found?.kind) {
      case 'Int':
        return Value.literal(new Json('int', found.value));
      case 'Float':
        return Value.literal(Json.float(found.value));
      case 'String':
        return Value.literal(Json.str(found.value));
      case 'Name':
        return Value.literal(nameLiteral(found.value));
      default:
        return this.compositeValue(found, depth);
    }
  }

  compositeValue(found, depth) {
    const punctuator = found?.value;
    if (punctuator === '$') return Value.variable(this.name());
    if (punctuator === '[') {
      const items = [];
      while (!this.eat(']')) items.push(this.value(depth + 1));
      return Value.list(items);
    }
    if (punctuator === '{') {
      const pairs = [];
      while (!this.eat('}')) {
        const name = this.name();
        this.expect(':');
        pairs.push([name, this.value(depth + 1)]);
      }
      return Value.object(pairs);
    }
    throw new Error(`Expected a value, found ${debugToken(found)}.`);
  }
}

/** A bare name as a value: a boolean, null, or an enum value travelling as its name. */
function nameLiteral(name) {
  if (name === 'true') return Json.bool(true);
  if (name === 'false') return Json.bool(false);
  if (name === 'null') return Json.NULL;
  return Json.str(name);
}
