// Checks the `DoubletsStorage` snippet of js/server/README.md: a transaction
// over the file-backed doublets storage, then the links read back, and a
// u32-addressed store that adopts the resolution decorators.
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  AddressType,
  CommitMode,
  DoubletsStorage,
  FileTransitionLog,
  INFINITE_RETENTION,
  TransactionsDecorator,
} from '../../js/server/src/index.js';

const directory = mkdtempSync(join(tmpdir(), 'clink-doublets-'));
try {
  // --- README snippet ---
  const store = DoubletsStorage.openExclusive(join(directory, 'db.links'));
  const transactions = new TransactionsDecorator(
    store,
    FileTransitionLog.open(join(directory, 'db.transitions.log')),
    INFINITE_RETENTION,
    CommitMode.Sync,
  );
  transactions.beginTransaction();
  const point = transactions.create(0, 0);
  transactions.update(point, point, point);
  transactions.commit();
  console.log(store.allLinks().map((link) => [link.index, link.source, link.target])); // [[1, 1, 1]]
  transactions.logStore().close();
  store.close();
  // --- end ---

  const narrow = DoubletsStorage.open(join(directory, 'u32.links'), {
    addressType: AddressType.U32,
  }).withAutomaticUniquenessAndUsagesResolution();
  narrow.createLink(1, 1);
  // An unflushed write changes the file too, as a write to a Rust mapping
  // does; `flush` publishes it and makes it the known revision.
  const before = narrow.hasExternalChanges();
  narrow.flush();
  console.log(narrow.linksCount(), before, narrow.hasExternalChanges()); // 1 true false
  narrow.close();
} finally {
  rmSync(directory, { recursive: true, force: true });
}
