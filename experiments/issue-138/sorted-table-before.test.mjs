import assert from 'node:assert/strict';
import test from 'node:test';
import { SortedJournalTable } from '../src/engine-sorted-table.js';
import { Device } from './support/sorted-table.mjs';
const rows = [{ key:'a',generation:'1',entries:['\ufeff雪\0😀\n'] },{key:'b',generation:'18446744073709551615',entries:null},{key:'c',generation:'3',entries:[]}];
test('sealed sorted table roundtrips tombstones and owns returned values',()=>{
 const device=new Device();const table=SortedJournalTable.create(device,rows);
 assert.equal(table.count,3);assert.equal(table.supportsDurableFlush,true);
 for(const row of rows) assert.deepEqual(table.get(row.key),row);
 assert.equal(table.get('absent'),undefined);assert.deepEqual(table.entries(),rows);
 table.get('a').entries[0]='caller';assert.deepEqual(table.get('a'),rows[0]);
 assert.deepEqual(SortedJournalTable.open(device.crash()).entries(),rows);
});
