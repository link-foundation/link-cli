// Shared fixtures of the GraphQL tests: the recorded files of
// docs/protocol/graphql, a Deep store in a scratch directory and a way to ask
// it a query, as the Rust tests under rust/tests/graphql_*.rs build them.
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

import { execute, Settings } from '../../src/graphql-execute.js';
import { Json } from '../../src/graphql-json.js';
import { DeepStore } from '../../src/graphql-store.js';
import { NamedTypesDecorator } from '../../src/named-types.js';
import { PermissionPolicy } from '../../src/permissions.js';
import { QueryProcessor } from '../../src/query-processor.js';
import { scratchDirectory } from './storage.mjs';

const DOCS = new URL('../../../../docs/protocol/graphql/', import.meta.url);

/** The path of a recorded file of docs/protocol/graphql. */
export const fixturePath = (name) => new URL(name, DOCS);

/** The text of a recorded file of docs/protocol/graphql. */
export const fixture = (name) => readFileSync(fixturePath(name), 'utf8');

export const GUEST = Object.freeze({ user: null, admin: false });
export const ADMIN = Object.freeze({ user: null, admin: true });
export const user = (id) => ({ user: BigInt(id), admin: false });

/** The settings of the tests: the secret `secret`, and `adminSecret` if given. */
export const settingsOf = (adminSecret = null) => new Settings(Buffer.from('secret'), adminSecret);

/**
 * The markers of the permission layer and `rule`, written as
 * `clink --auto-create-missing-references` writes them, without a policy.
 */
export function writeRules(path, rule) {
  const links = NamedTypesDecorator.open(path);
  new QueryProcessor(false)
    .withAutoCreateMissingReferences(true)
    .processQuery(
      links,
      `() ((Type: Type Type) (Extends: Extends Extends) (AllowRead: AllowRead AllowRead) (AllowWrite: AllowWrite AllowWrite) (Anything: Anything Anything) (Doc: Type Doc) ${rule})`,
    );
  links.save();
}

/**
 * A Deep store over a fresh database: `{ store, links, path }`. `rule` puts
 * it under the policy of that rule; `core` loads deep-core.json first.
 */
export function deepStore({ rule = null, core = false } = {}) {
  const path = join(scratchDirectory(), 'db.links');
  if (rule !== null) writeRules(path, rule);
  const links = NamedTypesDecorator.open(path).withIndexedLookups();
  const policy = rule === null ? null : PermissionPolicy.inData(links);
  const store = DeepStore.open(links, policy);
  if (core) store.loadPackage(Json.parse(fixture('deep-core.json')));
  return { store, links, path };
}

/** The response to `query`, as text. */
export function ask(store, settings, query, auth = GUEST, operation = null, variables = Json.NULL) {
  return execute(store, settings, query, operation, variables, auth)[0].write();
}
