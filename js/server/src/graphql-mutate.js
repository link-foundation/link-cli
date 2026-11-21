// `insert_T`, `update_T` and `delete_T` on `links` and the tables of values.
// Port of rust/src/graphql/mutate.rs.
//
// The methods are mixed into `Exec` (graphql-execute.js). Ids are BigInts; a
// sum past an i64 wraps, as a release build of Rust does.

import { GqlError } from './graphql-error.js';
import { Json } from './graphql-json.js';
import { primaryKey } from './graphql-select.js';
import { linkRowJson, ValueKind } from './graphql-store.js';

const writable = (table) => table === 'links' || ValueKind.ofTable(table) !== null;

/** Rust's `trim_end_matches`: every trailing `suffix` goes. */
function trimEndAll(text, suffix) {
  let rest = text;
  while (rest.endsWith(suffix)) rest = rest.slice(0, rest.length - suffix.length);
  return rest;
}

/** The table and whether it is `_by_pk`, of what follows `update_` or `delete_`. */
function target(rest) {
  return rest.endsWith('_by_pk') ? [rest.slice(0, -'_by_pk'.length), true] : [rest, false];
}

/** `id as u32` in Rust. */
const u32 = (id) => Number(BigInt.asUintN(32, id));

/** The id `given` fixes for `column`, or `null`. */
const givenId = (given, column) => given.find(([name]) => name === column)?.[1] ?? null;

/** Runs a write of the store; what it throws is an error of the request. */
function write(run) {
  try {
    return run();
  } catch (error) {
    throw GqlError.write(error);
  }
}

export const mutateMethods = {
  /** The mutation `field` names, `null` when it names none. */
  mutationField(field) {
    const name = field.name;
    if (name.startsWith('insert_') && writable(trimEndAll(name.slice('insert_'.length), '_one'))) {
      return this.insertField(field, name.slice('insert_'.length));
    }
    for (const prefix of ['update_', 'delete_']) {
      if (!name.startsWith(prefix)) continue;
      const [table, byPk] = target(name.slice(prefix.length));
      if (!writable(table)) continue;
      return prefix === 'update_'
        ? this.updateField(table, field, byPk)
        : this.deleteField(table, field, byPk);
    }
    if (name === 'reserve') {
      const count = field.argument('count', this.variables)?.asI64() ?? 0n;
      const ids = Json.array(this.store.reserve(count).map((id) => new Json('int', id)));
      return Json.object(
        this.flatten(field.selections, 'ReserveResponse').map((item) => [
          item.responseKey(),
          item.name === '__typename' ? Json.str('ReserveResponse') : ids,
        ]),
      );
    }
    return null;
  },

  insertField(field, rest) {
    const single = rest.endsWith('_one');
    const table = trimEndAll(rest, '_one');
    let objects;
    if (single) objects = [field.argument('object', this.variables) ?? Json.NULL];
    else {
      const given = field.argument('objects', this.variables);
      objects = given === null ? [] : given.kind === 'array' ? given.value : [given];
    }
    const rows = objects.map((object) => this.insert(table, object, []));
    this.memo.clear();
    if (!single) return this.response(table, rows, field);
    // One object, one row.
    return this.project(table, rows[0], field.selections);
  },

  updateField(table, field, byPk) {
    const rows = this.targetRows(table, field, byPk);
    const updated = rows.map((row) => this.update(table, row, field));
    this.memo.clear();
    return this.finish(table, updated, field, byPk);
  },

  deleteField(table, field, byPk) {
    const rows = this.targetRows(table, field, byPk);
    // What is returned is read before it is gone.
    const response = this.finish(table, rows, field, byPk);
    for (const row of rows) {
      const id = row.get('id').asI64();
      if (table === 'links') {
        // Taken along by an earlier row of this delete, it is done.
        if (this.store.tripletOf(id) !== null) {
          write(() => this.store.deleteLink(this.subject, id));
        }
      } else if (this.store.value(u32(id)) !== null) {
        write(() => this.store.deleteValue(this.subject, u32(id)));
      }
    }
    this.memo.clear();
    return response;
  },

  targetRows(table, field, byPk) {
    let condition;
    if (byPk) {
      const pk = primaryKey(table);
      const key =
        field.argument('pk_columns', this.variables)?.get(pk) ??
        field.argument(pk, this.variables) ??
        Json.NULL;
      condition = Json.object([[pk, Json.object([['_eq', key]])]]);
    } else condition = field.argument('where', this.variables) ?? Json.NULL;
    return this.rows(table, condition);
  },

  finish(table, rows, field, byPk) {
    if (!byPk) return this.response(table, rows, field);
    return rows.length > 0 ? this.project(table, rows[0], field.selections) : Json.NULL;
  },

  /** `{affected_rows returning {..}}`. */
  response(table, rows, field) {
    const typename = `${table}_mutation_response`;
    const out = [];
    for (const item of this.flatten(field.selections, typename)) {
      let value;
      if (item.name === '__typename') value = Json.str(typename);
      else if (item.name === 'affected_rows') value = Json.int(rows.length);
      else if (item.name === 'returning') value = this.projectAll(table, rows, item.selections);
      else {
        throw GqlError.validation(
          `field '${item.name}' not found in type: '${typename}'`,
          `$.selectionSet.${field.name}.${item.name}`,
        );
      }
      out.push([item.responseKey(), value]);
    }
    return Json.object(out);
  },

  /** The id a column or its nested `{data: ..}` object gives. */
  columnId(object, column, relationName, given) {
    const fixed = givenId(given, column);
    if (fixed !== null) return fixed;
    const nested = object.get(relationName)?.get('data') ?? null;
    if (nested !== null) return this.insert('links', nested, []).get('id').asI64();
    return object.get(column)?.asI64() ?? 0n;
  },

  /**
   * Inserts one object and its nested objects; `given` are the columns the
   * parent of a nested array fixes, as `[column, id]`.
   */
  insert(table, object, given) {
    const kind = ValueKind.ofTable(table);
    if (kind !== null) {
      const owner = givenId(given, 'link_id') ?? object.get('link_id')?.asI64() ?? 0n;
      const value = object.get('value') ?? Json.NULL;
      const row = write(() => this.store.createValue(this.subject, kind, owner, value));
      return this.valueJson(row, this.store.value(row));
    }
    const typeId = this.columnId(object, 'type_id', 'type', given);
    const fromId = this.columnId(object, 'from_id', 'from', given);
    const toId = this.columnId(object, 'to_id', 'to', given);
    const explicit = object.get('id')?.asI64() ?? null;
    const id = write(() => this.store.createLink(this.subject, explicit, typeId, fromId, toId));
    for (const [key, valueKind] of [
      ['string', ValueKind.String],
      ['number', ValueKind.Number],
      ['object', ValueKind.Object],
    ]) {
      const nested = object.get(key)?.get('data') ?? null;
      if (nested !== null) this.insert(valueKind, nested, [['link_id', id]]);
    }
    for (const [key, column] of [
      ['in', 'to_id'],
      ['out', 'from_id'],
      ['typed', 'type_id'],
    ]) {
      const nested = object.get(key)?.get('data') ?? null;
      if (nested === null) continue;
      const items = nested.kind === 'array' ? nested.value : [nested];
      for (const item of items) this.insert('links', item, [[column, id]]);
    }
    return linkRowJson({ id, typeId, fromId, toId });
  },

  update(table, row, field) {
    const set = field.argument('_set', this.variables) ?? Json.NULL;
    const increase = field.argument('_inc', this.variables) ?? Json.NULL;
    // Rows of `links` and of values always have their columns.
    const id = row.get('id').asI64();
    if (table === 'links') {
      const column = (name) => {
        const base = set.get(name)?.asI64() ?? row.get(name).asI64();
        return BigInt.asIntN(64, base + (increase.get(name)?.asI64() ?? 0n));
      };
      const next = {
        id,
        typeId: column('type_id'),
        fromId: column('from_id'),
        toId: column('to_id'),
      };
      write(() => this.store.updateLink(this.subject, id, next.typeId, next.fromId, next.toId));
      return linkRowJson(next);
    }
    const current = row.get('value');
    const replaced = set.get('value');
    const step = increase.get('value');
    let value = current;
    if (replaced !== null) value = replaced;
    else if (step !== null) value = Json.float((current.asF64() ?? 0) + (step.asF64() ?? 0));
    write(() => this.store.updateValue(this.subject, u32(id), value));
    return this.valueJson(u32(id), this.store.value(u32(id)));
  },
};
