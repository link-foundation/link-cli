import assert from 'node:assert/strict';
import { SortedJournalTable } from '../src/engine-sorted-table.js';
import { FilePageDevice } from '../src/engine-file-paged-log.js';
const [path,mode]=process.argv.slice(2);
assert.ok(path && ['create','read'].includes(mode),'Provide <dedicated-file> create|read');
const rows=Array.from({length:80},(_,i)=>({key:'k'+String(i).padStart(3,'0'),generation:i===1?'18446744073709551615':String(i+1),entries:i===1?null:i===2?[]:['\ufeff雪\0😀\n'+'x'.repeat(500)]}));
const device=new FilePageDevice(path);
try {
 const table=mode==='create'?SortedJournalTable.create(device,rows):SortedJournalTable.open(device);
 assert.deepEqual(table.entries(),rows);for(const row of rows)assert.deepEqual(table.get(row.key),row);assert.equal(table.get('missing'),undefined);
 console.log(`JS native sorted table ${mode}: ${table.count} exact rows, tombstone/empty/Unicode/u64 and indexed points; digest ${table.digest}`);
} finally {device.close();}
