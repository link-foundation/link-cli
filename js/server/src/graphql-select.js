// Reading tables: `where` over plans and checks, ordering, relations,
// aggregates and the projection of a selection.
// Port of rust/src/graphql/select.rs.
//
// The methods are mixed into `Exec` (graphql-execute.js); `this` is the run
// of one request. A row is a `Json` object, ids are BigInts and addresses of
// the store are numbers.

import { GqlError } from './graphql-error.js';
import { f64ToI64, Json } from './graphql-json.js';
import { compile } from './graphql-plan.js';
import { linkRowJson, ValueKind } from './graphql-store.js';
import { columns, compare, passes, relation, TABLES } from './graphql-tables.js';
import { matchTripletsQuery, TripletQuery } from './triplets.js';

/** The column `T_by_pk` and `update_T_by_pk` look rows up by. */
export function primaryKey(table) {
  if (table === 'selectors') return 'item_id';
  if (table === 'handlers') return 'handler_id';
  if (table === 'can') return 'rule_id';
  return 'id';
}

const stripSuffix = (name, suffix) =>
  name.endsWith(suffix) ? name.slice(0, name.length - suffix.length) : null;

const notFound = (name, typename, path) =>
  GqlError.validation(`field '${name}' not found in type: '${typename}'`, path);

const nonNegative = (value) => (value > 0n ? value : 0n);

// Ids of rows are unique; the sign of the difference is the order.
const byId = (left, right) => Number(left.id - right.id);

/** `max`, `min`, `sum` or `avg` of a column over rows. */
export function summarise(operation, rows, column) {
  const values = rows.map((row) => row.get(column)).filter((v) => v !== null && !v.isNull());
  if (values.length === 0) return Json.NULL;
  if (operation === 'max' || operation === 'min') {
    const wanted = operation === 'max' ? 1 : -1;
    return values.reduce((best, value) => (compare(value, best) === wanted ? value : best));
  }
  // Rust's `Sum` of `f64` starts at negative zero.
  let sum = -0;
  for (const value of values) sum += value.asF64() ?? -0;
  const result = operation === 'avg' ? sum / values.length : sum;
  if (Number.isInteger(result) && operation === 'sum') return new Json('int', f64ToI64(result));
  return Json.float(result);
}

function orderItems(order) {
  if (order.kind === 'array') return order.value;
  if (order.kind === 'null') return [];
  return [order];
}

function flattenOrder(item, path, specs) {
  if (item.kind !== 'object') return;
  for (const [key, value] of item.value) {
    path.push(key);
    const direction = value.asStr();
    if (direction !== null) specs.push([[...path], direction]);
    else flattenOrder(value, path, specs);
    path.pop();
  }
}

/** Postgres's defaults: ascending puts nulls last and descending first. */
export function orderValues(left, right, direction) {
  const descending = direction.startsWith('desc');
  const nullsFirst =
    direction.endsWith('nulls_first') || (descending && !direction.endsWith('nulls_last'));
  if (left.isNull() && right.isNull()) return 0;
  if (left.isNull()) return nullsFirst ? -1 : 1;
  if (right.isNull()) return nullsFirst ? 1 : -1;
  const found = compare(left, right) ?? 0;
  return descending ? -found || 0 : found;
}

const union = (left, right) => {
  for (const item of right) left.add(item);
  return left;
};

export const selectMethods = {
  /** The root fields `T`, `T_by_pk` and `T_aggregate`; `null` when the field names none. */
  selectField(field) {
    const name = field.name;
    if (TABLES.includes(name)) {
      const rows = this.select(name, field);
      return this.projectAll(name, rows, field.selections);
    }
    const keyed = stripSuffix(name, '_by_pk');
    if (keyed !== null && TABLES.includes(keyed)) {
      const key = primaryKey(keyed);
      const id = field.argument(key, this.variables) ?? Json.NULL;
      const condition = Json.object([[key, Json.object([['_eq', id]])]]);
      const rows = this.rows(keyed, condition);
      return rows.length > 0 ? this.project(keyed, rows[0], field.selections) : Json.NULL;
    }
    const aggregated = stripSuffix(name, '_aggregate');
    if (aggregated !== null && TABLES.includes(aggregated)) {
      return this.aggregate(aggregated, this.select(aggregated, field), field);
    }
    return null;
  },

  /** The rows the arguments of `field` select. */
  select(table, field) {
    const condition = field.argument('where', this.variables) ?? Json.NULL;
    return this.shape(table, this.rows(table, condition), field);
  },

  /** Applies `order_by`, `distinct_on`, `offset` and `limit`. */
  shape(table, rows, field) {
    const distinctOn = field.argument('distinct_on', this.variables) ?? Json.NULL;
    const distinct =
      distinctOn.kind === 'string'
        ? [distinctOn.value]
        : distinctOn
            .asList()
            .map((column) => column.asStr())
            .filter((column) => column !== null);
    const order = field.argument('order_by', this.variables) ?? Json.NULL;
    const specs = [];
    for (const item of orderItems(order)) flattenOrder(item, [], specs);
    let shaped = rows;
    if (specs.length > 0) {
      const keyed = shaped.map((row) => [
        specs.map(([path]) => this.sortValue(table, row, path)),
        row,
      ]);
      keyed.sort(([left], [right]) => {
        for (let at = 0; at < specs.length; at += 1) {
          const found = orderValues(left[at], right[at], specs[at][1]);
          if (found !== 0) return found;
        }
        return 0;
      });
      shaped = keyed.map(([, row]) => row);
    }
    if (distinct.length > 0) {
      const seen = new Set();
      shaped = shaped.filter((row) => {
        const key = JSON.stringify(distinct.map((column) => row.get(column)?.write() ?? ''));
        if (seen.has(key)) return false;
        seen.add(key);
        return true;
      });
    }
    const offset = nonNegative(field.argument('offset', this.variables)?.asI64() ?? 0n);
    shaped = shaped.slice(Number(offset));
    const limit = field.argument('limit', this.variables)?.asI64() ?? null;
    return limit === null ? shaped : shaped.slice(0, Number(nonNegative(limit)));
  },

  sortValue(table, row, path) {
    if (path.length === 0) return Json.NULL;
    const [first, ...rest] = path;
    const found = relation(table, first);
    if (found !== null) {
      const related = this.related(found, row.get(found.local));
      return related.length > 0 ? this.sortValue(found.target, related[0], rest) : Json.NULL;
    }
    return this.columnValue(table, row, first);
  },

  /** The rows of `table` passing `condition`. */
  rows(table, condition) {
    if (table !== 'links') {
      return this.tableRows(table).filter((row) => this.matches(table, row, condition));
    }
    const { plan, exact } = compile(condition, (position, id) => this.store.address(position, id));
    const found = this.linkRowsOf(plan).sort(byId);
    const rows = [];
    for (const link of found) {
      const row = linkRowJson(link);
      if (exact || this.matches('links', row, condition)) rows.push(row);
    }
    return rows;
  },

  linkRowsOf(plan) {
    const addresses = this.addresses(plan);
    const triplets = this.store.view(this.subject, (view) =>
      addresses.map((address) => view.getTriplet(address)).filter((triplet) => triplet != null),
    );
    return triplets.map((triplet) => this.store.rowOf(triplet)).filter((row) => row !== null);
  },

  /**
   * The addresses of the rows a plan stands for, ascending: this is where a
   * `where` becomes substitution queries.
   */
  addresses(plan) {
    return [...this.addressSet(plan)].sort((left, right) => left - right);
  },

  addressSet(plan) {
    const isRow = (address) => this.store.deepOf(address) !== null;
    switch (plan.kind) {
      case 'empty':
        return new Set();
      case 'all': {
        const all = this.store.view(this.subject, (view) => view.eachTriplet(TripletQuery.any()));
        return new Set(all.map((triplet) => triplet.index).filter(isRow));
      }
      case 'ids':
        return new Set(plan.ids.map((id) => this.store.tripletOf(id)).filter((a) => a !== null));
      case 'match': {
        let found;
        try {
          found = this.store.view(this.subject, (view) =>
            matchTripletsQuery(view, plan.text, '$i'),
          );
        } catch (error) {
          throw GqlError.write(error);
        }
        return new Set(found.filter(isRow));
      }
      case 'union':
        return plan.items.reduce((all, item) => union(all, this.addressSet(item)), new Set());
      case 'intersect': {
        let all = null;
        for (const item of plan.items) {
          const next = this.addressSet(item);
          all = all === null ? next : new Set([...all].filter((address) => next.has(address)));
        }
        return all ?? new Set();
      }
      default: {
        const without = this.addressSet(plan.item);
        const all = this.addressSet({ kind: 'all' });
        return new Set([...all].filter((address) => !without.has(address)));
      }
    }
  },

  /** A column, or what a computed field `value` of `links` holds. */
  columnValue(table, row, column) {
    if (table === 'links' && column === 'value') {
      // A row of `links` always has its id.
      const id = row.get('id').asI64();
      for (const kind of ValueKind.ALL) {
        const found = this.store.valueOfOwner(kind, id);
        if (found !== null) {
          return Json.object([
            ['id', Json.int(found[0])],
            ['value', found[1].value],
          ]);
        }
      }
      return Json.NULL;
    }
    return row.get(column) ?? Json.NULL;
  },

  /** Whether a row passes a `bool_exp`. */
  matches(table, row, condition) {
    if (condition.kind !== 'object') return true;
    for (const [key, value] of condition.value) {
      if (!this.term(table, row, key, value)) return false;
    }
    return true;
  },

  term(table, row, key, value) {
    if (key === '_and') return value.asList().every((item) => this.matches(table, row, item));
    if (key === '_or') return value.asList().some((item) => this.matches(table, row, item));
    if (key === '_not') return !this.matches(table, row, value);
    const found = relation(table, key);
    if (found !== null) {
      const set = this.remoteSet(found, value);
      const local = row.get(found.local);
      return local !== null && set.has(local.write());
    }
    if ((table === 'links' && key === 'value') || columns(table).includes(key)) {
      const column = this.columnValue(table, row, key);
      if (value.kind !== 'object') {
        throw GqlError.validation('a comparison is an object', `$.where.${key}`);
      }
      return value.value.every(([operator, argument]) => {
        try {
          return passes(column, operator, argument);
        } catch (error) {
          throw GqlError.validation(error.message, `$.where.${key}`);
        }
      });
    }
    throw notFound(key, `${table}_bool_exp`, `$.where.${key}`);
  },

  remoteSet(found, condition) {
    const key = `${found.target}|${found.remote}|${condition.write()}`;
    const kept = this.memo.get(key);
    if (kept !== undefined) return kept;
    const set = new Set(
      this.rows(found.target, condition)
        .map((row) => row.get(found.remote))
        .filter((value) => value !== null && !value.isNull())
        .map((value) => value.write()),
    );
    this.memo.set(key, set);
    return set;
  },

  /**
   * The rows at the far end of a relation, from the value of its local column.
   * Every row has its columns, and none of them is null.
   */
  related(found, local) {
    const id = local.asI64() ?? 0n;
    if (found.target === 'links') {
      const address = this.store.tripletOf(id);
      if (address === null) return [];
      let query = TripletQuery.byTo(address);
      if (found.remote === 'id') query = TripletQuery.byIndex(address);
      else if (found.remote === 'type_id') query = TripletQuery.byType(address);
      else if (found.remote === 'from_id') query = TripletQuery.byFrom(address);
      const triplets = this.store.view(this.subject, (view) => view.eachTriplet(query));
      return triplets
        .map((triplet) => this.store.rowOf(triplet))
        .filter((row) => row !== null)
        .sort(byId)
        .map(linkRowJson);
    }
    const kind = ValueKind.ofTable(found.target);
    if (kind !== null && found.remote === 'link_id') {
      const value = this.store.valueOfOwner(kind, id);
      return value === null ? [] : [this.valueJson(value[0], value[1])];
    }
    const wanted = local.write();
    return this.tableRows(found.target).filter((row) => row.get(found.remote)?.write() === wanted);
  },

  projectAll(table, rows, selections) {
    return Json.array(rows.map((row) => this.project(table, row, selections)));
  },

  /** The rows of a relation that pass the `where` of `field`. */
  relatedWhere(found, row, field) {
    const rows = this.related(found, row.get(found.local));
    const condition = field.argument('where', this.variables);
    if (condition === null) return rows;
    return rows.filter((candidate) => this.matches(found.target, candidate, condition));
  },

  /** A row under a selection. */
  project(table, row, selections) {
    const out = [];
    for (const field of this.flatten(selections, table)) {
      out.push([field.responseKey(), this.projectField(table, row, field)]);
    }
    return Json.object(out);
  },

  projectField(table, row, field) {
    const name = field.name;
    if (name === '__typename') return Json.str(table);
    const found = relation(table, name);
    if (found !== null) {
      const kept = this.relatedWhere(found, row, field);
      if (found.many) {
        return this.projectAll(
          found.target,
          this.shape(found.target, kept, field),
          field.selections,
        );
      }
      return kept.length > 0 ? this.project(found.target, kept[0], field.selections) : Json.NULL;
    }
    const base = stripSuffix(name, '_aggregate');
    const aggregated = base === null ? null : relation(table, base);
    if (aggregated !== null) {
      const kept = this.relatedWhere(aggregated, row, field);
      return this.aggregate(aggregated.target, this.shape(aggregated.target, kept, field), field);
    }
    if (
      columns(table).includes(name) ||
      (table === 'links' && name === 'value') ||
      row.get(name) !== null
    ) {
      return this.columnValue(table, row, name);
    }
    throw notFound(name, table, `$.selectionSet.${table}.selectionSet.${name}`);
  },

  /** `{aggregate {count max {..} ..} nodes {..}}`. */
  aggregate(table, rows, field) {
    const typename = `${table}_aggregate`;
    const out = [];
    for (const item of this.flatten(field.selections, typename)) {
      let value;
      if (item.name === '__typename') value = Json.str(typename);
      else if (item.name === 'nodes') value = this.projectAll(table, rows, item.selections);
      else if (item.name === 'aggregate') value = this.aggregateFields(table, rows, item);
      else throw notFound(item.name, typename, `$.selectionSet.${field.name}.${item.name}`);
      out.push([item.responseKey(), value]);
    }
    return Json.object(out);
  },

  aggregateFields(table, rows, field) {
    const typename = `${table}_aggregate_fields`;
    const out = [];
    for (const item of this.flatten(field.selections, typename)) {
      const name = item.name;
      let value;
      if (name === '__typename') value = Json.str(typename);
      else if (name === 'count') value = this.count(rows, item);
      else if (['max', 'min', 'sum', 'avg'].includes(name)) {
        const inner = `${table}_${name}_fields`;
        value = Json.object(
          this.flatten(item.selections, inner).map((column) => [
            column.responseKey(),
            column.name === '__typename' ? Json.str(inner) : summarise(name, rows, column.name),
          ]),
        );
      } else throw notFound(name, typename, `$.selectionSet.${field.name}.${name}`);
      out.push([item.responseKey(), value]);
    }
    return Json.object(out);
  },

  count(rows, item) {
    const selected = item.argument('columns', this.variables) ?? Json.NULL;
    const chosen = selected.asList();
    if (chosen.length === 0) return Json.int(rows.length);
    const key = (row) => {
      const parts = chosen
        .map((column) => column.asStr())
        .filter((column) => column !== null)
        .map((column) => row.get(column))
        .filter((value) => value !== null && !value.isNull())
        .map((value) => value.write());
      return parts.length === chosen.length ? parts.join('|') : null;
    };
    const present = rows.map(key).filter((found) => found !== null);
    const distinct = item.argument('distinct', this.variables);
    const unique = distinct !== null && distinct.kind === 'bool' && distinct.value;
    return Json.int(unique ? new Set(present).size : present.length);
  },
};
