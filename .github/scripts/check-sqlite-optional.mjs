// Resolve actual production dependencies; optional test/example providers are
// intentionally outside the core package boundary inspected here.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { registerHooks } from 'node:module';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const run = (command, args) => execFileSync(command, args, {
  cwd: root, encoding: 'utf8', stdio: ['ignore', 'pipe', 'inherit'],
});
const mode = process.argv[2];
if (mode === 'rust') {
  const tree = (manifest, extra = []) => run('cargo', [
    'tree', '--manifest-path', manifest, '--locked', '--edges', 'normal', '--prefix', 'none', ...extra,
  ]);
  const sqlite = /^(?:rusqlite|libsqlite3-sys) v/gm;
  const configurations = [
    ['default native', 'rust/Cargo.toml', []],
    ['bare native', 'rust/Cargo.toml', ['--no-default-features']],
    ['native pages', 'rust/Cargo.toml', ['--no-default-features', '--features', 'native-pages']],
    ['sorted tables', 'rust/Cargo.toml', ['--no-default-features', '--features', 'native-pages,sorted-tables']],
    ['physical LSM components', 'rust/Cargo.toml', ['--no-default-features', '--features', 'physical-lsm']],
    ['default WASM', 'rust/wasm/Cargo.toml', ['--target', 'wasm32-unknown-unknown']],
    ['WASM with native-only feature', 'rust/Cargo.toml', ['--target', 'wasm32-unknown-unknown', '--features', 'sqlite,native-pages']],
  ];
  for (const [label, manifest, extra] of configurations) {
    const resolved = tree(manifest, extra);
    assert.equal([...resolved.matchAll(sqlite)].length, 0, `${label} unexpectedly resolves SQLite:\n${resolved}`);
    if (label.includes('WASM')) assert.doesNotMatch(resolved, /^(?:getrandom|libc|windows-sys) v/m, `${label} resolves native page I/O`);
    console.log(`${label}: no SQLite production dependency`);
  }
  const selected = tree('rust/Cargo.toml', ['--features', 'sqlite']);
  assert.equal([...selected.matchAll(sqlite)].length, 2, 'explicit native sqlite must select its provider');
  console.log('explicit native sqlite: provider selected');
} else if (mode === 'csharp') {
  const report = JSON.parse(run('dotnet', [
    'list', 'csharp/Foundation.Data.Doublets.Cli.Library/Foundation.Data.Doublets.Cli.Library.csproj',
    'package', '--include-transitive', '--format', 'json', '--no-restore',
  ]));
  assert.equal(report.projects.length, 1);
  const packages = report.projects.flatMap((project) => project.frameworks.flatMap(
    (framework) => [...framework.topLevelPackages, ...(framework.transitivePackages ?? [])],
  ));
  assert.ok(packages.length > 0, 'dependency inspection returned no core packages');
  assert.deepEqual(packages.filter((item) => /sqlite/i.test(item.id)), []);
  console.log(`C# core: ${packages.length} resolved packages, no SQLite provider`);
} else if (mode === 'js') {
  const hook = registerHooks({
    resolve(specifier, context, nextResolve) {
      assert.doesNotMatch(specifier, /sqlite/i, `unexpected SQLite module load: ${specifier}`);
      return nextResolve(specifier, context);
    },
  });
  const directory = mkdtempSync(join(tmpdir(), 'clink-optional-'));
  try {
    const { MemoryCommitLog, ReplicatedCommitLog } = await import('../../js/client/src/index.js');
    const { FileCommitLog } = await import('../../js/client/src/engine-file-log.js');
    const { FilePagedCommitLog, FilePageDevice } = await import('../../js/client/src/engine-file-paged-log.js');
    const { SortedJournalTable } = await import('../../js/client/src/engine-sorted-table.js');
    const { LsmWal } = await import('../../js/client/src/engine-lsm-wal.js');
    const { FileMirroredPagedCommitLog } = await import('../../js/client/src/engine-file-mirrored-pages.js');
    const { FileCheckpointPagedCommitLog } = await import('../../js/client/src/engine-file-checkpoint-pages.js');
    await assert.rejects(import('../../js/client/src/engine-sqlite-log.js'), /unexpected SQLite module load/);
    const replica = new MemoryCommitLog();
    const memory = new ReplicatedCommitLog(new MemoryCommitLog(), [replica], { acknowledgements: 2 });
    memory.append('native 雪');
    memory.flush();
    assert.deepEqual(JSON.parse(replica.read()), ['native 雪']);
    const path = join(directory, 'journal');
    using journal = new FileCommitLog(path);
    journal.append('native 雪');
    journal.flush();
    journal.close();
    using reopened = new FileCommitLog(path);
    assert.deepEqual(JSON.parse(reopened.read()), ['native 雪']);
    const pagePath = join(directory, 'native-pages');
    using pages = new FilePagedCommitLog(pagePath);
    pages.append('native \ufeff\0雪'); pages.flush(); pages.close();
    using reopenedPages = new FilePagedCommitLog(pagePath);
    assert.deepEqual(JSON.parse(reopenedPages.read()), ['native \ufeff\0雪']);
    using tableDevice = new FilePageDevice(join(directory, 'sorted-table'));
    const tableRows = [{ key: 'a', generation: '18446744073709551615', entries: null }];
    assert.deepEqual(SortedJournalTable.create(tableDevice, tableRows).entries(), tableRows);
    assert.deepEqual(SortedJournalTable.open(tableDevice).entries(), tableRows);
    const lsmCheckpoint = '["native-lsm1","0","0","0",[],[]]';
    assert.equal(LsmWal.encode(LsmWal.recover([lsmCheckpoint])), lsmCheckpoint);
    const mirroredPath = join(directory, 'mirrored-pages');
    using mirrored = new FileMirroredPagedCommitLog(mirroredPath);
    mirrored.append('mirrored 雪'); mirrored.flush(); mirrored.close();
    using reopenedMirror = new FileMirroredPagedCommitLog(mirroredPath);
    assert.deepEqual(JSON.parse(reopenedMirror.read()), ['mirrored 雪']);
    for (const mirroredBanks of [false, true]) for (const mirroredRoot of [false, true]) {
      const checkpointPath = join(directory, `checkpoint-${mirroredBanks}-${mirroredRoot}`);
      using checkpoint = new FileCheckpointPagedCommitLog(checkpointPath, { mirroredBanks, mirroredRoot });
      assert.equal(checkpoint.replace(['checkpoint \ufeff\0雪']), true); checkpoint.close();
      using reopenedCheckpoint = new FileCheckpointPagedCommitLog(checkpointPath, { mirroredBanks, mirroredRoot });
      assert.deepEqual(JSON.parse(reopenedCheckpoint.read()), ['checkpoint \ufeff\0雪']);
    }
    console.log('JS root, memory replication and native file/page reopening: no SQLite module loads');
  } finally {
    hook.deregister();
    rmSync(directory, { recursive: true, force: true });
  }
} else {
  throw new Error('Usage: node check-sqlite-optional.mjs rust|csharp|js');
}
