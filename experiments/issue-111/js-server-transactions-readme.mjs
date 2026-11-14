// Checks the transactions snippet of js/server/README.md: one committed and
// one rolled-back transaction, then a reopen that sees only the first.
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { NamedTypesDecorator, TransactionsDecorator } from '../../js/server/src/index.js';

const directory = mkdtempSync(join(tmpdir(), 'clink-tx-'));
const db = join(directory, 'db.links');
try {
  // --- README snippet ---
  const open = () =>
    new TransactionsDecorator(
      NamedTypesDecorator.open(db),
      NamedTypesDecorator.open(TransactionsDecorator.makeTransitionsDatabaseFilename(db)),
    );
  const store = open();
  store.beginTransaction();
  store.create(1, 1);
  store.commit();
  store.beginTransaction();
  store.create(2, 2);
  store.rollback();
  store.save();
  console.log(
    open()
      .all()
      .map((link) => [link.index, link.source, link.target]),
  ); // [ [ 1, 1, 1 ] ]
  // --- end ---
} finally {
  rmSync(directory, { recursive: true, force: true });
}
