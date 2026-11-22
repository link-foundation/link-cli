// Tables Deep derives from links with triggers and views. Here they are
// computed from the links when asked, for as long as nothing is written:
// values and `can` always, `tree` and `selectors` from the `Tree*` and
// `Selector*` links, `mp` the same paths under its own column names.
// `handlers` is empty, as Deep's handlers do not run here.
// Port of rust/src/graphql/virtual_tables.rs.
//
// The methods are mixed into `Exec` (graphql-execute.js).

import { Json } from './graphql-json.js';
import { ValueKind } from './graphql-store.js';
import { TripletQuery } from './triplets.js';

/**
 * Which `(parent, child)` edges a link of an included type makes, over its
 * `from`, itself and its `to`. These are the groups of include types Deep's
 * `isAllowSpread*` conditions test, in `1621815803572-materialized-path.ts`.
 */
const SPREADS = [
  [
    'from-link',
    ['TreeIncludeDown', 'TreeIncludeIn', 'TreeIncludeFromCurrent', 'TreeIncludeFromCurrentTo'],
  ],
  [
    'to-link',
    ['TreeIncludeUp', 'TreeIncludeIn', 'TreeIncludeToCurrent', 'TreeIncludeToCurrentFrom'],
  ],
  [
    'link-to',
    ['TreeIncludeDown', 'TreeIncludeOut', 'TreeIncludeCurrentTo', 'TreeIncludeFromCurrentTo'],
  ],
  [
    'link-from',
    ['TreeIncludeUp', 'TreeIncludeOut', 'TreeIncludeCurrentFrom', 'TreeIncludeToCurrentFrom'],
  ],
];

/** Every other include type: it only makes a link a member of its tree. */
const MEMBERSHIP_ONLY = ['TreeIncludeNode'];

/** Ids in order; the ids sorted are never equal. */
const ascending = (left, right) => Number(left - right);

/** The list kept under `key`, made empty when there is none yet. */
function entry(map, key, make) {
  let found = map.get(key);
  if (found === undefined) {
    found = make();
    map.set(key, found);
  }
  return found;
}

/** A row of `strings`, `numbers` or `objects`. */
export function valueJson(row, value) {
  return Json.object([
    ['id', Json.int(row)],
    ['link_id', new Json('int', value.owner)],
    ['value', value.value],
  ]);
}

/**
 * The rows of every path to each of `nodes`. Deep refuses cycles when they
 * are written; here a link already on a path ends it.
 */
export function pushPaths(rows, tree, nodes, parents) {
  const walk = (node, path, out) => {
    path.push(node);
    const above = (parents.get(node) ?? []).filter((parent) => !path.includes(parent));
    if (above.length === 0) out.push([...path].reverse());
    for (const parent of above) walk(parent, path, out);
    path.pop();
  };
  for (const node of nodes) {
    const paths = [];
    walk(node, [], paths);
    paths.forEach((path, position) => {
      path.forEach((parent, depth) => {
        rows.push(
          Json.object([
            ['id', Json.int(rows.length + 1)],
            ['link_id', new Json('int', node)],
            ['tree_id', new Json('int', tree)],
            ['root_id', new Json('int', path[0])],
            ['parent_id', new Json('int', parent)],
            ['depth', Json.int(depth)],
            ['position_id', Json.str(`${tree}-${node}-${position}`)],
            ['self', Json.bool(parent === node)],
          ]),
        );
      });
    });
  }
}

/** The edge a link of an included type makes, as `[parent, child]`. */
function edge(spread, link) {
  if (spread === 'from-link') return [link.fromId, link.id];
  if (spread === 'to-link') return [link.toId, link.id];
  if (spread === 'link-to') return [link.id, link.toId];
  return [link.id, link.fromId];
}

export const virtualTableMethods = {
  valueJson,

  /** The row of a triplet below permissions, for the server's own lookups. */
  rawRow(triplet) {
    const admin = this.store.admin;
    this.store.admin = true;
    const found = this.store.view(0, (view) => view.getTriplet(triplet));
    this.store.admin = admin;
    return found == null ? null : this.store.rowOf(found);
  },

  allLinks() {
    const admin = this.store.admin;
    this.store.admin = true;
    const all = this.store.view(0, (view) => view.eachTriplet(TripletQuery.any()));
    this.store.admin = admin;
    return all.map((triplet) => this.store.rowOf(triplet)).filter((row) => row !== null);
  },

  /** Every row of a table that is not `links`, as the subject sees them. */
  tableRows(table) {
    const kind = ValueKind.ofTable(table);
    if (kind !== null) {
      // A value is seen with its owner.
      const policy = this.store.hasPolicy();
      const seen = (value) => {
        if (!policy) return true;
        const address = this.store.tripletOf(value.owner);
        return (
          address !== null &&
          this.store.view(this.subject, (view) => view.getTriplet(address)) != null
        );
      };
      return this.store
        .values()
        .filter(([, value]) => value.kind === kind && seen(value))
        .map(([row, value]) => valueJson(row, value));
    }
    if (table === 'can') return this.canRows();
    if (table === 'tree') return this.derived('tree', () => this.treeRows());
    if (table === 'selectors') return this.derived('selectors', () => this.selectorRows());
    if (table === 'mp') return this.mpRows();
    if (table === 'healthz') return [Json.object([['status', Json.str('ok')]])];
    return [];
  },

  /** A table computed from the links, kept until the next write. */
  derived(table, compute) {
    const generation = this.store.generation;
    const kept = this.store.derived.get(table);
    if (!this.store.inMutation && kept !== undefined && kept.generation === generation) {
      return [...kept.rows];
    }
    const rows = compute();
    if (!this.store.inMutation) this.store.derived.set(table, { generation, rows: [...rows] });
    return rows;
  },

  /**
   * The policy's grants as Deep's `can`: a rule on `Anything` stands as the
   * id `0` rather than one row for each link it reaches.
   */
  canRows() {
    const select = this.coreId('AllowSelect');
    const update = this.coreId('AllowUpdate');
    return this.store.grants().map(([write, subject, object]) =>
      Json.object([
        ['rule_id', Json.int(0)],
        ['action_id', new Json('int', (write ? update : select) ?? 0n)],
        ['object_id', new Json('int', object ?? 0n)],
        ['subject_id', new Json('int', subject ?? 0n)],
      ]),
    );
  },

  /**
   * `tree`: every path from a root to a link, one row per link on it, as
   * Deep's materialized path stores them. `typesTree` is built in: a link's
   * parent is its type.
   */
  treeRows() {
    const links = this.allLinks();
    const any = this.coreId('Any');
    const typesTree = this.coreId('typesTree');
    const spreads = new Map();
    for (const [spread, names] of SPREADS) {
      for (const name of names) {
        const id = this.coreId(name);
        if (id !== null) entry(spreads, id, () => []).push(spread);
      }
    }
    for (const name of MEMBERSHIP_ONLY) {
      const id = this.coreId(name);
      if (id !== null) entry(spreads, id, () => []);
    }
    // tree -> included type -> the edges its links make.
    const trees = new Map();
    for (const link of links) {
      const made = spreads.get(link.typeId);
      if (link.fromId === typesTree || made === undefined) continue;
      const includes = entry(trees, link.fromId, () => new Map());
      const edges = entry(includes, link.toId, () => new Set());
      for (const spread of made) edges.add(spread);
    }
    const rows = [];
    if (typesTree !== null) {
      const parents = new Map();
      for (const link of links) {
        if (link.typeId !== link.id && link.typeId !== 0n) {
          entry(parents, link.id, () => []).push(link.typeId);
        }
      }
      pushPaths(
        rows,
        typesTree,
        links.map((link) => link.id),
        parents,
      );
    }
    for (const tree of [...trees.keys()].sort(ascending)) {
      const includes = trees.get(tree);
      const wanted = (typeId) => includes.has(typeId) || (any !== null && includes.has(any));
      const members = new Set(links.filter((link) => wanted(link.typeId)).map((link) => link.id));
      const parents = new Map();
      for (const link of links.filter((candidate) => members.has(candidate.id))) {
        for (const spread of includes.get(link.typeId) ?? []) {
          const [parent, child] = edge(spread, link);
          if (parent !== child && members.has(parent) && members.has(child)) {
            const list = entry(parents, child, () => []);
            if (!list.includes(parent)) list.push(parent);
          }
        }
      }
      // The rows do not depend on the order the includes were found in.
      for (const list of parents.values()) list.sort(ascending);
      pushPaths(rows, tree, [...members].sort(ascending), parents);
    }
    return rows;
  },

  /** `mp`: the rows of `tree`, as Deep's own table names their columns. */
  mpRows() {
    const renamed = [
      ['id', 'id'],
      ['item_id', 'link_id'],
      ['path_item_id', 'parent_id'],
      ['path_item_depth', 'depth'],
      ['root_id', 'root_id'],
      ['position_id', 'position_id'],
      ['group_id', 'tree_id'],
    ];
    return this.derived('tree', () => this.treeRows()).map((row) =>
      Json.object(renamed.map(([name, column]) => [name, row.get(column)])),
    );
  },

  /** `selectors`: what a `SelectorInclude` reaches down its tree. */
  selectorRows() {
    const include = this.coreId('SelectorInclude');
    const treeLink = this.coreId('SelectorTree');
    if (include === null || treeLink === null) return [];
    const filter = this.coreId('SelectorFilter');
    const links = this.allLinks();
    const treeRows = this.derived('tree', () => this.treeRows());
    // A row of `tree` has every column.
    const int = (row, key) => row.get(key).asI64();
    const reached = new Map();
    for (const row of treeRows) {
      const key = `${int(row, 'tree_id')}|${int(row, 'parent_id')}`;
      entry(reached, key, () => []).push(int(row, 'link_id'));
    }
    const treesOf = new Map();
    const queryOf = new Map();
    for (const link of links) {
      if (link.typeId === treeLink) entry(treesOf, link.fromId, () => []).push(link.toId);
      else if (link.typeId === filter) queryOf.set(link.fromId, link.toId);
    }
    const rows = [];
    for (const link of links.filter((candidate) => candidate.typeId === include)) {
      for (const tree of treesOf.get(link.id) ?? []) {
        for (const item of reached.get(`${tree}|${link.toId}`) ?? []) {
          rows.push(
            Json.object([
              ['item_id', new Json('int', item)],
              ['selector_id', new Json('int', link.fromId)],
              ['selector_include_id', new Json('int', link.id)],
              ['query_id', new Json('int', queryOf.get(link.fromId) ?? 0n)],
            ]),
          );
        }
      }
    }
    return rows;
  },
};
