// Hasura's `links_bool_exp` as substitution queries.
// Port of rust/src/graphql/plan.rs.
//
// The part of a `where` that is a conjunction of equalities and relations
// becomes one restriction over joined variables:
// `{from: {type_id: {_eq: 5}}}` is `(($i: * $v1 *) ($v1: 5 * *))`. `_in` and
// `_or` are unions of such queries, `_and` their intersection and `_not` what
// the rest leaves. What a restriction cannot say (`_gt`, `_ilike`, values) is
// left to the executor, which checks every candidate the plan found.
//
// A plan is `{ kind }` of `all`, `empty`, `match` (with `text`), `ids` (with
// BigInt `ids`), `union` and `intersect` (with `items`) or `minus` (with
// `item`). A mapping is `(position, id) => address | null`, where position
// is `'index'`, `'type'` or `'other'`: type `0` is a triplet of its own,
// because a triplet's type cannot be null.

import { Json } from './graphql-json.js';

export const Position = Object.freeze({ Index: 'index', Type: 'type', Other: 'other' });

/** One triplet pattern under construction; a part is `null` while it is `*`. */
class Node {
  constructor(index) {
    this.index = index;
    this.parts = [null, null, null];
    this.impossible = false;
  }

  render() {
    const part = (slot) => this.parts[slot] ?? '*';
    return `(${this.index}: ${part(0)} ${part(1)} ${part(2)})`;
  }

  set(slot, value) {
    const found = this.parts[slot];
    if (found !== null && found !== value) this.impossible = true;
    else this.parts[slot] = value;
  }

  constraints() {
    return this.parts.filter((part) => part !== null && !part.startsWith('$')).length;
  }
}

const SLOTS = { type_id: 0, from_id: 1, to_id: 2 };
/** `from: {...}`: the column of this link holding the related link. */
const FORWARD = { type: 0, from: 1, to: 2 };
/** `in: {...}`: the column of the related link holding this link. */
const BACKWARD = { typed: 0, out: 1, in: 2 };

const slotIn = (table, key) => (Object.hasOwn(table, key) ? table[key] : null);

function singleEq(condition) {
  if (condition.kind !== 'object' || condition.value.length !== 1) return null;
  const [key, value] = condition.value[0];
  return key === '_eq' ? value.asI64() : null;
}

class Builder {
  constructor(mapping) {
    this.mapping = mapping;
    this.nodes = [new Node('$i')];
    this.nextVariable = 0;
  }

  variable() {
    this.nextVariable += 1;
    return `$v${this.nextVariable}`;
  }

  address(position, id) {
    if (id === 0n && position === Position.Other) return '0';
    const address = this.mapping(position, id);
    return address === null ? null : String(address);
  }

  /**
   * Whether `where` is wholly a conjunction this restriction can say. When
   * it is not, nothing is kept: the caller looks at it another way.
   */
  pure(node, condition) {
    const mark = this.nodes.length;
    const saved = [...this.nodes[node].parts];
    if (condition.kind !== 'object') return false;
    const pure =
      condition.value.length > 0 &&
      condition.value.every(([key, value]) => this.term(node, key, value));
    if (!pure) {
      this.nodes.length = mark;
      this.nodes[node].parts = saved;
    }
    return pure;
  }

  /** Adds the term to `node`; false when the restriction cannot say it. */
  term(node, key, value) {
    const slot = slotIn(SLOTS, key);
    if (slot !== null) {
      const id = singleEq(value);
      if (id === null) return false;
      const address = this.address(slot === 0 ? Position.Type : Position.Other, id);
      if (address === null) this.nodes[node].impossible = true;
      else this.nodes[node].set(slot, address);
      return true;
    }
    const forward = slotIn(FORWARD, key);
    if (forward !== null) {
      const variable = this.variable();
      const child = this.nodes.length;
      this.nodes.push(new Node(variable));
      if (this.pure(child, value)) {
        this.nodes[node].set(forward, variable);
        return true;
      }
      this.nodes.length = child;
      return false;
    }
    const backward = slotIn(BACKWARD, key);
    if (backward !== null) {
      const pattern = new Node(this.variable());
      const child = this.nodes.length;
      pattern.parts[backward] = this.nodes[node].index;
      this.nodes.push(pattern);
      if (this.pure(child, value)) return true;
      this.nodes.length = child;
      return false;
    }
    if (key === '_and') {
      return value
        .asList()
        .every(
          (item) =>
            item.kind === 'object' &&
            item.value.every(([name, inner]) => this.term(node, name, inner)),
        );
    }
    return false;
  }
}

/** Plans a `where` over `links`: `{ plan, exact }`, exact when the plan says all it does. */
export function compile(condition, mapping) {
  if (condition.kind !== 'object') return { plan: { kind: 'all' }, exact: true };
  const builder = new Builder(mapping);
  const plans = [];
  let exact = true;
  for (const [key, value] of condition.value) {
    const ids = key === 'id' ? idList(value) : null;
    if (ids !== null) {
      plans.push({ kind: 'ids', ids });
      continue;
    }
    const mark = builder.nodes.length;
    const saved = [...builder.nodes[0].parts];
    if (builder.term(0, key, value)) continue;
    builder.nodes.length = mark;
    builder.nodes[0].parts = saved;
    const [plan, whole] = other(key, value, mapping);
    if (plan !== null) plans.push(plan);
    exact &&= whole;
  }
  const constrained =
    builder.nodes.length > 1 || builder.nodes[0].parts.some((part) => part !== null);
  if (builder.nodes.some((node) => node.impossible))
    return { plan: { kind: 'empty' }, exact: true };
  if (constrained) {
    const [root, ...rest] = builder.nodes;
    // The most constrained first: each next pattern starts from bindings.
    const children = rest.sort((left, right) => right.constraints() - left.constraints());
    const ordered = root.constraints() > 0 ? [root, ...children] : [...children, root];
    plans.unshift({ kind: 'match', text: `(${ordered.map((node) => node.render()).join(' ')})` });
  }
  if (plans.length === 0) return { plan: { kind: 'all' }, exact };
  return { plan: plans.length === 1 ? plans[0] : { kind: 'intersect', items: plans }, exact };
}

/** The ids `{_eq: id}` or `{_in: [ids]}` names, as BigInts, or `null`. */
export function idList(condition) {
  if (condition.kind !== 'object' || condition.value.length !== 1) return null;
  const [key, value] = condition.value[0];
  if (key === '_eq') {
    const id = value.asI64();
    return id === null ? null : [id];
  }
  if (key !== '_in' || value.kind !== 'array') return null;
  const ids = value.value.map((item) => item.asI64());
  return ids.includes(null) ? null : ids;
}

const compileAll = (value, mapping) => value.asList().map((item) => compile(item, mapping));

/**
 * A term outside the conjunction: a plan narrowing the rows, if there is one,
 * and whether that plan says all the term does.
 */
function other(key, value, mapping) {
  if (key === '_or' || key === '_and') {
    const items = compileAll(value, mapping);
    if (key === '_or' && items.length === 0) return [null, false];
    const plan = { kind: key === '_or' ? 'union' : 'intersect', items: items.map((i) => i.plan) };
    return [plan, items.every((item) => item.exact)];
  }
  if (key === '_not') {
    const inner = compile(value, mapping);
    return inner.exact ? [{ kind: 'minus', item: inner.plan }, true] : [null, false];
  }
  const ids = slotIn(SLOTS, key) === null ? null : idList(value);
  if (ids === null) return [null, false];
  const branches = ids.map(
    (id) =>
      compile(Json.object([[key, Json.object([['_eq', new Json('int', id)]])]]), mapping).plan,
  );
  return [{ kind: 'union', items: branches }, true];
}

/** A plan as text, for tests and `--trace`. */
export function render(plan) {
  const list = (items) => items.map(render).join(' ');
  switch (plan.kind) {
    case 'all':
    case 'empty':
      return plan.kind;
    case 'match':
      return plan.text;
    case 'ids':
      return `(ids ${plan.ids.join(' ')})`;
    case 'union':
    case 'intersect':
      return `(${plan.kind} ${list(plan.items)})`;
    default:
      return `(minus ${render(plan.item)})`;
  }
}
