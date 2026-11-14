// Probes Deno.FsFile locking. Run: deno run --allow-all experiments/issue-111/storage-lock-deno-api.mjs
const dir = Deno.makeTempDirSync();
const path = `${dir}/x.lock`;
const a = Deno.openSync(path, { read: true, write: true, create: true });
const b = Deno.openSync(path, { read: true, write: true, create: true });
console.log('methods', ['lockSync', 'tryLockSync', 'unlockSync'].map((m) => typeof a[m]));
a.lockSync(true);
console.log('b.tryLockSync(true)', b.tryLockSync(true), 'b.tryLockSync(false)', b.tryLockSync(false));
a.unlockSync();
console.log('after unlock b.tryLockSync(true)', b.tryLockSync(true));
console.log('rid/fd', a.rid, Object.keys(a));
a.close();
b.close();
try { const d = Deno.openSync(dir, { read: true, write: true, create: true }); console.log(d) } catch (e) { console.log('dir error', e.name, e.code, e.message); }
