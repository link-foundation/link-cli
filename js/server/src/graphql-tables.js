// The tables of the Deep schema: their relations, as Hasura's metadata
// declares them, and how rows are compared.
// Port of rust/src/graphql/tables.rs.
//
// Rust matches `_regex` with the `regex_lite` crate; here a pattern runs as
// a JavaScript `RegExp` in Unicode mode, and what `regex_lite` cannot build
// (look-around, back-references, `\p` classes) matches nothing, as a pattern
// that does not build matches nothing there. `regex_lite` folds case for
// ASCII only, a `RegExp` for all of Unicode.

import { compareCodePoints } from './link-reference-validator.js';

const one = (local, target, remote) => ({ local, target, remote, many: false });
const many = (local, target, remote) => ({ local, target, remote, many: true });

export const TABLES = [
  'links',
  'strings',
  'numbers',
  'objects',
  'can',
  'tree',
  'selectors',
  'handlers',
  'mp',
  'healthz',
];

const VALUE_LINK = one('link_id', 'links', 'id');

const RELATIONS = {
  links: {
    from: one('from_id', 'links', 'id'),
    to: one('to_id', 'links', 'id'),
    type: one('type_id', 'links', 'id'),
    string: one('id', 'strings', 'link_id'),
    number: one('id', 'numbers', 'link_id'),
    object: one('id', 'objects', 'link_id'),
    in: many('id', 'links', 'to_id'),
    out: many('id', 'links', 'from_id'),
    typed: many('id', 'links', 'type_id'),
    can_action: many('id', 'can', 'action_id'),
    can_object: many('id', 'can', 'object_id'),
    can_rule: many('id', 'can', 'rule_id'),
    can_subject: many('id', 'can', 'subject_id'),
    down: many('id', 'tree', 'parent_id'),
    up: many('id', 'tree', 'link_id'),
    tree: many('id', 'tree', 'tree_id'),
    root: many('id', 'tree', 'root_id'),
    selected: many('id', 'selectors', 'selector_id'),
    selectors: many('id', 'selectors', 'item_id'),
    _by_group: many('id', 'mp', 'group_id'),
    _by_item: many('id', 'mp', 'item_id'),
    _by_path_item: many('id', 'mp', 'path_item_id'),
    _by_root: many('id', 'mp', 'root_id'),
  },
  strings: { link: VALUE_LINK },
  numbers: { link: VALUE_LINK },
  objects: { link: VALUE_LINK },
  can: {
    action: one('action_id', 'links', 'id'),
    object: one('object_id', 'links', 'id'),
    rule: one('rule_id', 'links', 'id'),
    subject: one('subject_id', 'links', 'id'),
  },
  tree: {
    link: one('link_id', 'links', 'id'),
    parent: one('parent_id', 'links', 'id'),
    root: one('root_id', 'links', 'id'),
    tree: one('tree_id', 'links', 'id'),
    by_tree: one('tree_id', 'links', 'id'),
    by_link: many('link_id', 'tree', 'link_id'),
    by_parent: many('parent_id', 'tree', 'parent_id'),
    by_position: many('position_id', 'tree', 'position_id'),
    by_root: many('root_id', 'tree', 'root_id'),
  },
  selectors: {
    item: one('item_id', 'links', 'id'),
    selector: one('selector_id', 'links', 'id'),
    query: many('query_id', 'links', 'id'),
  },
  handlers: {
    dist: one('dist_id', 'links', 'id'),
    execution_provider: one('execution_provider_id', 'links', 'id'),
    handler: one('handler_id', 'links', 'id'),
    isolation_provider: one('isolation_provider_id', 'links', 'id'),
    src: one('src_id', 'links', 'id'),
  },
};

/** The relation `name` of `table`, or `null`: `{ local, target, remote, many }`. */
export function relation(table, name) {
  const relations = Object.hasOwn(RELATIONS, table) ? RELATIONS[table] : {};
  return Object.hasOwn(relations, name) ? relations[name] : null;
}

const VALUE_COLUMNS = ['id', 'link_id', 'value'];

const COLUMNS = {
  links: ['id', 'type_id', 'from_id', 'to_id'],
  strings: VALUE_COLUMNS,
  numbers: VALUE_COLUMNS,
  objects: VALUE_COLUMNS,
  can: ['rule_id', 'action_id', 'object_id', 'subject_id'],
  tree: ['id', 'link_id', 'tree_id', 'root_id', 'parent_id', 'depth', 'position_id', 'self'],
  selectors: ['item_id', 'selector_id', 'selector_include_id', 'query_id'],
  handlers: ['dist_id', 'src_id', 'handler_id', 'isolation_provider_id', 'execution_provider_id'],
  mp: ['id', 'item_id', 'path_item_id', 'path_item_depth', 'root_id', 'position_id', 'group_id'],
  healthz: ['status'],
};

export function columns(table) {
  return Object.hasOwn(COLUMNS, table) ? COLUMNS[table] : [];
}

/** Numbers compare as numbers, whatever way they were written: -1, 0, 1 or `null`. */
export function compare(left, right) {
  if (left.kind === 'string' && right.kind === 'string') {
    return Math.sign(compareCodePoints(left.value, right.value));
  }
  if (left.kind === 'bool' && right.kind === 'bool') {
    return Number(left.value) - Number(right.value);
  }
  const a = left.asF64();
  const b = right.asF64();
  if (a === null || b === null || Number.isNaN(a) || Number.isNaN(b)) return null;
  return Math.sign(a - b) || 0;
}

/** SQL's `LIKE`: `%` is any run and `_` any one character. */
export function like(value, wanted, insensitive) {
  const fold = (data) => Array.from(insensitive ? data.toLowerCase() : data);
  const text = fold(value);
  const pattern = fold(wanted);
  let at = 0;
  let position = 0;
  let retry = null;
  while (at < text.length) {
    const expected = pattern[position];
    if (expected === '%') {
      retry = [at, position];
      position += 1;
    } else if (expected !== undefined && (expected === '_' || expected === text[at])) {
      at += 1;
      position += 1;
    } else if (retry !== null) {
      const [from, star] = retry;
      at = from + 1;
      retry = [from + 1, star];
      position = star + 1;
    } else return false;
  }
  return pattern.slice(position).every((character) => character === '%');
}

/** What `regex_lite` refuses to build: look-around, back-references, Unicode classes. */
const UNSUPPORTED = /\(\?<?[=!]|(^|[^\\])(\\\\)*\\[1-9kpP]/;

export function regex(text, pattern, insensitive) {
  if (UNSUPPORTED.test(pattern)) return false;
  try {
    return new RegExp(pattern, insensitive ? 'iu' : 'u').test(text);
  } catch {
    return false;
  }
}

function contains(outer, inner) {
  if (outer.kind === 'object' && inner.kind === 'object') {
    return inner.value.every(([key, value]) => {
      const found = outer.get(key);
      return found !== null && contains(found, value);
    });
  }
  if (outer.kind === 'array' && inner.kind === 'array') {
    return inner.value.every((item) => outer.value.some((found) => contains(found, item)));
  }
  return compare(outer, inner) === 0 || outer.equals(inner);
}

const equal = (value, argument) => compare(value, argument) === 0;
const among = (value, argument) => argument.asList().some((item) => equal(value, item));

const OPERATORS = {
  _eq: (value, argument) => equal(value, argument) || value.equals(argument),
  _neq: (value, argument) =>
    !equal(value, argument) && !value.equals(argument) && !argument.isNull(),
  _gt: (value, argument) => compare(value, argument) === 1,
  _gte: (value, argument) => [0, 1].includes(compare(value, argument)),
  _lt: (value, argument) => compare(value, argument) === -1,
  _lte: (value, argument) => [-1, 0].includes(compare(value, argument)),
  _in: among,
  _nin: (value, argument) => !among(value, argument),
  _like: (value, argument, text, pattern) => like(text, pattern, false),
  _nlike: (value, argument, text, pattern) => !like(text, pattern, false),
  _ilike: (value, argument, text, pattern) => like(text, pattern, true),
  _nilike: (value, argument, text, pattern) => !like(text, pattern, true),
  _regex: (value, argument, text, pattern) => regex(text, pattern, false),
  _nregex: (value, argument, text, pattern) => !regex(text, pattern, false),
  _iregex: (value, argument, text, pattern) => regex(text, pattern, true),
  _niregex: (value, argument, text, pattern) => !regex(text, pattern, true),
  _contains: (value, argument) => contains(value, argument),
  _contained_in: (value, argument) => contains(argument, value),
  _has_key: (value, argument, text, pattern) => value.get(pattern) !== null,
};

/**
 * Whether a column's value passes `{_op: argument}`; SQL's null compares
 * false to everything but `_is_null`. An unknown operator throws its message.
 */
export function passes(value, operator, argument) {
  if (operator === '_is_null') {
    return value.isNull() === (argument.kind === 'bool' && argument.value === true);
  }
  if (value.isNull()) return false;
  if (!Object.hasOwn(OPERATORS, operator)) {
    throw new Error(`field '${operator}' not found in type: 'comparison_exp'`);
  }
  return OPERATORS[operator](value, argument, value.asStr() ?? '', argument.asStr() ?? '');
}
