// Bounded child for actual native mutation cuts; no product fault switches.
import fs from 'node:fs';
import { syncBuiltinESMExports } from 'node:module';
import { FileCheckpointPagedCommitLog } from '../../js/client/src/engine-file-checkpoint-pages.js';
const [path, mirror, cut, phase, rootMirror] = process.argv.slice(2), point = Number(cut), handles = new Map(), events = [];
let active = phase !== 'replace';
const open = fs.openSync;
fs.openSync = (name, ...options) => { const fd = open(name, ...options); if (name === path || name.startsWith(path + '.')) handles.set(fd, name.slice(path.length) || 'root'); return fd; };
for (const operation of ['writeSync', 'ftruncateSync', 'fsyncSync']) {
  const original = fs[operation];
  fs[operation] = (fd, ...args) => {
    const observe = after => {
      if (active && handles.has(fd)) { events.push(`${handles.get(fd)}:${operation}:${after}`); if (events.length - 1 === point) process.exit(73); }
    };
    observe(false); const result = original(fd, ...args); observe(true); return result;
  };
}
syncBuiltinESMExports();
using log = new FileCheckpointPagedCommitLog(path, { mirroredBanks: mirror === 'true', mirroredRoot: rootMirror === 'true', maxPages: 64 });
if (phase === 'replace') { active = true; log.replace(['雪'.repeat(700), '\ufeff\0😀']); }
active = false;
log.close(); process.stdout.write(JSON.stringify(events));
