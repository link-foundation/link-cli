// links-queue's spec encodes (5: 5 5) as 0F 05. Prints what its encoder writes.
//
//   node self-reference.mjs <links-queue checkout>
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const { BinaryNotation, HEADER_SIZE } = await import(
  pathToFileURL(resolve(process.argv[2], 'js/src/protocol/binary-notation.js')).href
);
const link = { id: 5, source: 5, target: 5 };
const body = BinaryNotation.encode(link).slice(HEADER_SIZE + 1); // after the frame and the count
console.log(`(5: 5 5) -> ${Buffer.from(body).toString('hex').replace(/../g, '$& ').trim()}`);
