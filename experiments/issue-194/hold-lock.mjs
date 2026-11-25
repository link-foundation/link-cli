// Holds the store lock of a database for a while: node hold-lock.mjs DB KIND MS
import { quietFfiWarning, StoreLock, parseStoreLockOptions } from '../../js/server/src/store-lock.js';
import { LockMode } from '../../js/server/src/storage-lock.js';
quietFfiWarning();
const [db, kind = 'flock', ms = '1500'] = process.argv.slice(2);
const lock = await StoreLock.acquire([db], LockMode.Exclusive, parseStoreLockOptions(kind, null));
console.log('held', lock.paths().join(' '));
await new Promise((r) => setTimeout(r, Number(ms)));
lock.release();
