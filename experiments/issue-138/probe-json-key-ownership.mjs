// Finite V8 backing-storage inspection; use --allow-natives-syntax.
// One 64-character key and one 64 KiB payload; heap is capped by the command.
// node --max-old-space-size=64 --stack-size=1024 --allow-natives-syntax experiments/issue-138/probe-json-key-ownership.mjs
// Compiled at run time so the file stays parseable without the V8 natives flag.
const debugPrint = new Function('value', '%DebugPrint(value)');
const key = 'key_'.repeat(16);
const row = JSON.parse(JSON.stringify([key, '1', ['x'.repeat(65536)]]));
console.log('JSON-decoded key:');
debugPrint(row[0]);
const copied = new TextDecoder().decode(new TextEncoder().encode(row[0]));
console.log('UTF-8-owned key:');
debugPrint(copied);
if (row[0] !== copied) throw Error('key content changed');
