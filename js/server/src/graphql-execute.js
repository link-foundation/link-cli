// Runs a GraphQL document: authorisation, the root fields and introspection.
// Port of rust/src/graphql/execute.rs.
//
// `GqlError` lives in graphql-error.js; the reading, writing and derived
// tables are mixed into `Exec` from graphql-select.js, graphql-mutate.js and
// graphql-virtual-tables.js, as Rust spreads one `impl Exec` over them.

import { randomBytes } from 'node:crypto';
import { readFileSync } from 'node:fs';

import { errorText } from './error-report.js';
import { signToken, verifyToken } from './graphql-crypto.js';
import { Document } from './graphql-document.js';
import { GqlError } from './graphql-error.js';
import { Json, trimRust } from './graphql-json.js';
import { mutateMethods } from './graphql-mutate.js';
import { selectMethods } from './graphql-select.js';
import { ValueKind } from './graphql-store.js';
import { virtualTableMethods } from './graphql-virtual-tables.js';

/** The recorded answer of Hasura to the introspection query, read once. */
let introspection = null;

function recordedSchema() {
  introspection ??= Json.parse(
    readFileSync(new URL('./graphql-introspection.json', import.meta.url), 'utf8'),
  ).get('data');
  return introspection;
}

/** Seconds since the epoch, as a BigInt. */
const now = () => BigInt(Math.floor(Date.now() / 1000));

/** What a server keeps for all its requests. */
export class Settings {
  /** `secret` signs tokens (bytes); `adminSecret` is `x-hasura-admin-secret` or `null`. */
  constructor(secret, adminSecret) {
    this.secret = secret;
    this.adminSecret = adminSecret;
    this.introspection = recordedSchema();
  }

  /** A secret for tokens that need not outlive this process. */
  static randomSecret() {
    return randomBytes(32);
  }

  /**
   * The subject of `Authorization: Bearer <jwt>` and of the admin secret:
   * `{ user, admin }`, `user` a BigInt or `null` for a guest. A refusal
   * throws its message.
   */
  authorize(authorization, adminSecret) {
    let admin = false;
    if (adminSecret != null) {
      if (this.adminSecret === null || this.adminSecret !== adminSecret) {
        throw new Error('invalid x-hasura-admin-secret');
      }
      admin = true;
    }
    const token =
      authorization == null
        ? ''
        : trimRust(authorization.startsWith('Bearer ') ? authorization.slice(7) : authorization);
    if (token === '') return { user: null, admin };
    try {
      return { user: BigInt.asIntN(64, verifyToken(this.secret, token, now())), admin };
    } catch (error) {
      throw new Error(`Could not verify JWT: ${error.message}`, { cause: error });
    }
  }
}

/** The store's answer to a commit or a rollback that failed, as a response. */
function failed(error) {
  return [GqlError.write({ message: errorText(error) }).toJson(), false];
}

/**
 * Runs an operation: `[response, wrote]`, the whole response and whether a
 * mutation was committed.
 */
export function execute(store, settings, query, operation, variables, auth) {
  let document;
  let selected;
  try {
    document = Document.parse(query);
    selected = document.operation(operation);
  } catch (error) {
    return [GqlError.validation(error.message, '$.query').toJson(), false];
  }
  // A variable a request leaves out takes the operation's default.
  const pairs = variables.kind === 'object' ? [...variables.value] : [];
  for (const [name, fallback] of selected.variables) {
    if (fallback !== null && variables.get(name) === null) {
      pairs.push([name, fallback.resolve(Json.NULL)]);
    }
  }
  store.admin = auth.admin;
  const subject = (auth.user === null ? null : store.tripletOf(auth.user)) ?? 0;
  const exec = new Exec(store, settings, document, Json.object(pairs), auth, subject);
  const mutation = selected.kind === 'mutation';
  if (mutation) store.begin();
  let response;
  try {
    const data = exec.root(selected.selections, mutation);
    if (mutation) {
      try {
        store.commit();
      } catch (error) {
        return failed(error);
      }
    }
    response = [Json.object([['data', data]]), mutation];
  } catch (error) {
    if (mutation) {
      try {
        store.rollback();
      } catch (undo) {
        return failed(undo);
      }
    }
    response = [error.toJson(), false];
  }
  store.admin = false;
  return response;
}

const SCHEMA_TYPES = new Map([
  ['queryType', '__Type'],
  ['mutationType', '__Type'],
  ['subscriptionType', '__Type'],
  ['types', '__Type'],
  ['type', '__Type'],
  ['ofType', '__Type'],
  ['interfaces', '__Type'],
  ['possibleTypes', '__Type'],
  ['fields', '__Field'],
  ['args', '__InputValue'],
  ['inputFields', '__InputValue'],
  ['enumValues', '__EnumValue'],
  ['directives', '__Directive'],
]);

/** One request's run over the store. */
export class Exec {
  constructor(store, settings, document, variables, auth, subject) {
    this.store = store;
    this.settings = settings;
    this.document = document;
    this.variables = variables;
    this.auth = auth;
    /** The subject's address in the store; 0 is a guest. */
    this.subject = subject;
    /** The remote columns of a relation's filter, kept for one root field. */
    this.memo = new Map();
  }

  /** The fields of `selections` that apply to `typename`, without fragments. */
  flatten(selections, typename) {
    const fields = [];
    this.collect(selections, typename, fields, 0);
    return fields;
  }

  included(directives) {
    return directives.every((directive) => {
      const found = directive.arguments.find(([name]) => name === 'if');
      const condition = found === undefined ? null : found[1].resolve(this.variables);
      if (condition === null || condition.kind !== 'bool') return true;
      if (directive.name === 'skip') return !condition.value;
      return directive.name !== 'include' || condition.value;
    });
  }

  collect(selections, typename, out, depth) {
    if (depth > 64) throw GqlError.validation('fragments are nested too deeply', '$.query');
    for (const selection of selections) {
      if (selection.kind === 'field') {
        const { field } = selection;
        const key = field.responseKey();
        if (this.included(field.directives) && !out.some((seen) => seen.responseKey() === key)) {
          out.push(field);
        }
      } else if (selection.kind === 'spread') {
        if (!this.included(selection.directives)) continue;
        const fragment = this.document.fragments.get(selection.name);
        if (fragment === undefined) {
          throw GqlError.validation(`fragment '${selection.name}' is undefined`, '$.query');
        }
        this.collect(fragment, typename, out, depth + 1);
      } else if (
        (selection.on === null || selection.on === typename) &&
        this.included(selection.directives)
      ) {
        this.collect(selection.selections, typename, out, depth + 1);
      }
    }
  }

  root(selections, mutation) {
    const typename = mutation ? 'mutation_root' : 'query_root';
    const data = [];
    for (const field of this.flatten(selections, typename)) {
      this.memo.clear();
      data.push([field.responseKey(), this.rootField(field, typename, mutation)]);
    }
    return Json.object(data);
  }

  rootField(field, root, mutation) {
    const name = field.name;
    if (name === '__typename') return Json.str(root);
    const found = mutation ? this.mutationField(field) : this.queryField(field);
    if (found !== null) return found;
    throw GqlError.validation(
      `field '${name}' not found in type: '${root}'`,
      `$.selectionSet.${name}`,
    );
  }

  queryField(field) {
    // The recorded schema has its `__schema`, whose types all have names.
    const schema = this.settings.introspection.get('__schema');
    switch (field.name) {
      case '__schema':
        return this.projectSchema(schema, field.selections, '__Schema');
      case '__type': {
        const wanted = field.argument('name', this.variables)?.asStr() ?? null;
        const types = schema.get('types').asList();
        const found = types.find((item) => item.get('name').asStr() === wanted);
        return this.projectSchema(found ?? Json.NULL, field.selections, '__Type');
      }
      case 'guest':
        return this.guest(field);
      case 'jwt':
        return this.jwt(field);
      default:
        return this.selectField(field);
    }
  }

  /** Selects from the recorded schema: its keys are the field names. */
  projectSchema(value, selections, typename) {
    if (value.kind === 'array') {
      return Json.array(value.value.map((item) => this.projectSchema(item, selections, typename)));
    }
    if (value.kind !== 'object') return value;
    const out = [];
    for (const field of this.flatten(selections, typename)) {
      const name = field.name;
      let found;
      if (name === '__typename') found = Json.str(typename);
      else {
        const inner = value.get(name) ?? Json.NULL;
        const next = SCHEMA_TYPES.get(name) ?? typename;
        found =
          field.selections.length === 0 ? inner : this.projectSchema(inner, field.selections, next);
      }
      out.push([field.responseKey(), found]);
    }
    return Json.object(out);
  }

  tokenResponse(field, typename, id, error) {
    const token =
      id === null ? null : signToken(this.settings.secret, BigInt.asUintN(64, id), now());
    const out = [];
    for (const item of this.flatten(field.selections, typename)) {
      let value;
      if (item.name === '__typename') value = Json.str(typename);
      else if (item.name === 'token') value = token === null ? Json.NULL : Json.str(token);
      else if (item.name === 'linkId') value = id === null ? Json.NULL : new Json('int', id);
      else if (item.name === 'error') value = error === null ? Json.NULL : Json.str(error);
      else {
        throw GqlError.validation(
          `field '${item.name}' not found in type: '${typename}'`,
          `$.selectionSet.${field.name}.${item.name}`,
        );
      }
      out.push([item.responseKey(), value]);
    }
    return Json.object(out);
  }

  /**
   * `guest`: a new link of type `User`, with a token for it. Who asks does
   * not matter, so the layer below is asked as the server.
   */
  guest(field) {
    this.store.begin();
    const admin = this.store.admin;
    this.store.admin = true;
    let id;
    try {
      id = this.createGuest();
    } catch (error) {
      this.store.admin = admin;
      try {
        this.store.rollback();
      } catch (undo) {
        throw GqlError.write({ message: errorText(undo) });
      }
      throw GqlError.write(error);
    }
    this.store.admin = admin;
    try {
      this.store.commit();
    } catch (error) {
      throw GqlError.write({ message: errorText(error) });
    }
    return this.tokenResponse(field, 'GuestOutput', id, null);
  }

  createGuest() {
    const user = this.coreId('User');
    const [id] = this.store.reserve(1n);
    this.store.createLink(0, id, user ?? id, 0n, 0n);
    // A guest of a database with `deep/users` is one of the users.
    const join = this.coreId('Join');
    const users = join === null ? null : this.containedIn('deep', 'users');
    if (users !== null) this.store.createLink(0, null, join, users, id);
    return id;
  }

  /**
   * `jwt(input: {linkId})`: a token for the asker or, with permission, for
   * the link named; no link names the asker, as a "who am I" query.
   */
  jwt(field) {
    const asker = this.auth.user;
    const named = field.argument('input', this.variables)?.get('linkId')?.asI64() ?? null;
    const target = named ?? asker;
    if (target === null) {
      return this.tokenResponse(field, 'JWTOutput', null, 'No link and no subject.');
    }
    if (!this.auth.admin && this.store.hasPolicy() && asker !== target) {
      return this.tokenResponse(field, 'JWTOutput', null, 'Forbidden.');
    }
    if (this.store.tripletOf(target) === null) {
      return this.tokenResponse(field, 'JWTOutput', null, `No link ${target}.`);
    }
    return this.tokenResponse(field, 'JWTOutput', target, null);
  }

  /** The link a `Contain` named `name` holds from the package named `package`. */
  containedIn(pkg, name) {
    const contains = this.store
      .values()
      .filter(([, row]) => row.kind === ValueKind.String && row.value.asStr() === name)
      .map(([, row]) => row.owner);
    for (const contain of contains) {
      const triplet = this.store.tripletOf(contain);
      const row = triplet === null ? null : this.rawRow(triplet);
      const parent = row === null ? null : this.store.valueOfOwner(ValueKind.String, row.fromId);
      if (parent !== null && parent[1].value.asStr() === pkg) return row.toId;
    }
    return null;
  }

  /** The id of a type of `@deep-foundation/core`. */
  coreId(name) {
    return this.containedIn('@deep-foundation/core', name);
  }
}

Object.assign(Exec.prototype, selectMethods, mutateMethods, virtualTableMethods);
