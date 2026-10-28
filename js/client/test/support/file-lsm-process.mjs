// Actual abrupt-process experiment; finite inputs and bounded output.
import { FileLsmJournalStore } from '../../src/engine-file-lsm-store.js';
import { selected } from './lsm-store.mjs';
const [directory,stop,mirrored]=process.argv.slice(2);
if(!directory||!['open','group','flush','following','compact'].includes(stop)||!['0','1'].includes(mirrored))throw Error('finite child inputs');
const modes={mirroredRoot:mirrored==='1',mirroredBanks:mirrored==='1',mirroredTables:mirrored==='1'};
const store=new FileLsmJournalStore(directory,{...selected,maxPages:32,...modes});
function phase(name){process.stdout.write(name+'\n');if(stop===name)process.exit(73);}
phase('open');
if(!store.compareExchange(['a','b'],store.read(['a','b']),[['durable\0雪😀'],[]]))throw Error('group rejected');
phase('group');store.flush();phase('flush');
if(!store.compareExchange(['a'],store.read(['a']),[['following']]))throw Error('following group rejected');
phase('following');store.compact();phase('compact');
throw Error('unreached child boundary');
