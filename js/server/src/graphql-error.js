// An error as Hasura reports it, with the code and the path of the request it
// is about. Port of `GqlError` in rust/src/graphql/execute.rs, in a module of
// its own so the executor's parts can raise it without importing each other.

import { Json } from './graphql-json.js';

export class GqlError extends Error {
  constructor(message, code, path) {
    super(message);
    this.code = code;
    this.path = path;
  }

  /** A request that does not fit the schema. */
  static validation(message, path) {
    return new GqlError(message, 'validation-failed', path);
  }

  /** A write the store refused, coded by what its message says. */
  static write(error) {
    const message = error.message;
    let code = 'unexpected';
    if (message.includes('Forbidden')) code = 'permission-error';
    else if (message.includes('violation')) code = 'constraint-violation';
    return new GqlError(message, code, '$');
  }

  toJson() {
    return Json.object([
      [
        'errors',
        Json.array([
          Json.object([
            ['message', Json.str(this.message)],
            [
              'extensions',
              Json.object([
                ['path', Json.str(this.path)],
                ['code', Json.str(this.code)],
              ]),
            ],
          ]),
        ]),
      ],
    ]);
  }
}
