// The world of `@link-foundation/ui-links`' permission tests, the port of
// rust/tests/common/permissions_world.rs, shared by the permission tests:
//
//   Type -> User -> Admin              alice: User, boss: Admin
//   Type -> Doc -> Note -> Shared      Shared extends Public too
//   Type -> Doc -> Secret
//   Type -> Public
//
// Admins write anything, users write notes and read what is public.
import { LinkStorage } from '../src/link-storage.js';
import { Forbidden } from '../src/permissions-named-links.js';
import {
  PermissionAccess,
  PermissionMarkers,
  PermissionPolicy,
  PermissionsDecorator,
} from '../src/permissions.js';
import { TypesDecorator } from '../src/types.js';

const TYPES = [
  ['User', []],
  ['Admin', ['User']],
  ['Doc', []],
  ['Public', []],
  ['Note', ['Doc']],
  ['Shared', ['Note', 'Public']],
  ['Secret', ['Doc']],
];

const VALUES = [
  ['alice', 'User'],
  ['boss', 'Admin'],
  ['note', 'Note'],
  ['shared', 'Shared'],
  ['secret', 'Secret'],
];

/**
 * The world in `links`, its rules in `rules` or in `links` when `null`;
 * returns `{ t, v, markers, types }`, the types and values by name.
 */
export function makeWorld(links, rules = null) {
  const types = TypesDecorator.withNamedMarkers(links);
  const markers = PermissionMarkers.ensure(types);
  const t = {};
  for (const [name, parents] of TYPES) {
    const [parent, ...more] = parents.map((parentName) => t[parentName]);
    const id = parent == null ? types.defineRoot() : types.define(parent, more);
    types.setName(id, name);
    t[name] = id;
  }
  const v = {};
  for (const [name, typeName] of VALUES) {
    const id = types.instance(t[typeName]);
    types.setName(id, name);
    v[name] = id;
  }
  const target = rules ?? links;
  PermissionPolicy.allow(target, markers, PermissionAccess.Write, t.Admin, markers.anything);
  PermissionPolicy.allow(target, markers, PermissionAccess.Write, t.User, t.Note);
  PermissionPolicy.allow(target, markers, PermissionAccess.Read, t.User, t.Public);
  return { t, v, markers, types: types.markers() };
}

/** The world over in-memory stores, its rules in the data or in a store of their own. */
export class World {
  constructor(separateRules = false) {
    this.links = LinkStorage.inMemory();
    const rulesApart = LinkStorage.inMemory();
    const { t, v, markers, types } = makeWorld(this.links, separateRules ? rulesApart : null);
    this.policy = separateRules
      ? PermissionPolicy.withRules(this.links, rulesApart)
      : PermissionPolicy.inData(this.links);
    Object.assign(this, { t, v, markers, types });
  }

  /** The store as `subject` sees it. */
  as(subject) {
    return new PermissionsDecorator(this.links, this.policy, subject);
  }

  rules() {
    return this.policy.rulesOf(this.links);
  }

  allow(access, subject, object) {
    return PermissionPolicy.allow(this.rules(), this.markers, access, subject, object);
  }

  link(id) {
    const found = this.links.getLink(id);
    if (found == null) throw new Error(`no link ${id}`);
    return found;
  }
}

/** Every link of `links`, by index. */
export function all(links) {
  return [...links.allLinks()].sort((left, right) => left.index - right.index);
}

/** The indexes of every link of `links`, in order. */
export function indexes(links) {
  return all(links).map((value) => value.index);
}

/**
 * How `action` fails: `'forbidden'`, `'not-found'` or `'bad-request'`;
 * `null` when it does not.
 */
export function errorOf(action) {
  try {
    action();
    return null;
  } catch (error) {
    if (error instanceof Forbidden) return 'forbidden';
    if (String(error.message).startsWith('Link not found')) return 'not-found';
    return 'bad-request';
  }
}
