// Fresh loader checks both absence by default and explicit positive selection.
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
let tableLoads=0;
const hook=registerHooks({resolve(specifier,context,nextResolve){
 if(specifier.endsWith('/engine-sorted-table.js')){tableLoads++;throw Error('selected sorted-table codec');}
 assert.doesNotMatch(specifier,/sqlite/i);return nextResolve(specifier,context);
}});
try{
 const {MemoryCommitLog}=await import('../../js/client/src/index.js');const log=new MemoryCommitLog();log.append('ordinary');log.flush();assert.equal(tableLoads,0);
 await assert.rejects(import('../../js/client/src/engine-sorted-table.js'),/selected sorted-table codec/);assert.equal(tableLoads,1);
}finally{hook.deregister();}
console.log('PASS: universal root and ordinary memory log do not load sorted-table or SQLite codecs; explicit positive selection reaches the table codec.');
