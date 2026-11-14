// Interop of js/server/src/storage-lock.js with the Rust lock of
// rust/src/storage/lock.rs (std `File::lock` / `try_lock`, i.e. flock(2)) and with
// util-linux flock(1). Compiles a 30-line Rust program with rustc that opens the
// sidecar exactly as `FileLock::open` does, then checks contention both ways.
//
// node experiments/issue-111/storage-lock-rust-interop.mjs
// bun experiments/issue-111/storage-lock-rust-interop.mjs
// deno run --allow-all experiments/issue-111/storage-lock-rust-interop.mjs
import { spawn, spawnSync } from 'node:child_process';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { FileLock, LockMode, lockBackend } from '../../js/server/src/storage-lock.js';

const RUST = `
use std::fs::{OpenOptions, TryLockError};
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let file = OpenOptions::new().create(true).truncate(false).read(true).write(true)
        .open(&args[2]).unwrap();
    let shared = args.get(3).map(|m| m == "shared").unwrap_or(false);
    match args[1].as_str() {
        "try" => {
            let result = if shared { file.try_lock_shared() } else { file.try_lock() };
            match result {
                Ok(()) => println!("locked"),
                Err(TryLockError::WouldBlock) => println!("busy"),
                Err(TryLockError::Error(e)) => println!("error {e}"),
            }
        }
        _ => {
            file.lock().unwrap();
            println!("locked");
            std::thread::sleep(std::time::Duration::from_millis(600));
        }
    }
}
`;

const dir = mkdtempSync(join(tmpdir(), 'storage-lock-interop-'));
const lockPath = join(dir, 'db.links.lock');
const results = [];
const check = (name, actual, expected) => {
  results.push(`${actual === expected ? 'ok  ' : 'FAIL'} ${name}: ${actual}`);
};

console.log(`primitive: ${lockBackend().primitive ?? lockBackend().unavailable}`);
writeFileSync(join(dir, 'main.rs'), RUST);
const rustc = spawnSync('rustc', ['-O', '-o', join(dir, 'rust-lock'), join(dir, 'main.rs')], {
  encoding: 'utf8',
});
const rust = rustc.status === 0 ? join(dir, 'rust-lock') : null;
if (rust === null) console.log(`rustc unavailable: ${rustc.error?.message ?? rustc.stderr}`);

const run = (program, args) => spawnSync(program, args, { encoding: 'utf8' });

/** Starts `program` holding the lock; resolves once it says so. */
function holder(program, args, ready) {
  const child = spawn(program, args, { stdio: ['ignore', 'pipe', 'inherit'] });
  return new Promise((resolve) => {
    if (ready === null) setTimeout(() => resolve(child), 200);
    else child.stdout.on('data', (chunk) => String(chunk).includes(ready) && resolve(child));
  });
}
const exited = (child) => new Promise((resolve) => child.on('exit', resolve));

if (rust) {
  const held = FileLock.acquire(lockPath, LockMode.Exclusive);
  check('JS exclusive held, Rust try_lock', run(rust, ['try', lockPath]).stdout.trim(), 'busy');
  check('JS exclusive held, Rust try_lock_shared', run(rust, ['try', lockPath, 'shared']).stdout.trim(), 'busy');
  held.release();
  check('JS released, Rust try_lock', run(rust, ['try', lockPath]).stdout.trim(), 'locked');
  const shared = FileLock.acquire(lockPath, LockMode.Shared);
  check('JS shared held, Rust try_lock_shared', run(rust, ['try', lockPath, 'shared']).stdout.trim(), 'locked');
  check('JS shared held, Rust try_lock', run(rust, ['try', lockPath]).stdout.trim(), 'busy');
  shared.release();

  const child = await holder(rust, ['hold', lockPath], 'locked');
  check('Rust lock held, JS tryAcquire(Exclusive)', FileLock.tryAcquire(lockPath, LockMode.Exclusive), null);
  check('Rust lock held, JS tryAcquire(Shared)', FileLock.tryAcquire(lockPath, LockMode.Shared), null);
  const started = Date.now();
  const blocking = FileLock.acquire(lockPath, LockMode.Exclusive);
  check('JS acquire blocked until Rust exited', Date.now() - started > 100, true);
  blocking.release();
  await exited(child);
}

const flock = run('flock', ['--version']).status === 0;
if (flock) {
  const held = FileLock.acquire(lockPath, LockMode.Exclusive);
  check('JS exclusive held, flock -n exit status', run('flock', ['-n', lockPath, 'true']).status, 1);
  held.release();
  check('JS released, flock -n exit status', run('flock', ['-n', lockPath, 'true']).status, 0);
  const child = await holder('flock', [lockPath, 'sleep', '0.6'], null);
  check('flock(1) held, JS tryAcquire', FileLock.tryAcquire(lockPath, LockMode.Exclusive), null);
  await exited(child);
  const after = FileLock.tryAcquire(lockPath, LockMode.Exclusive);
  check('flock(1) exited, JS tryAcquire', after !== null, true);
  after?.release();
} else console.log('flock(1) unavailable');

console.log(results.join('\n'));
