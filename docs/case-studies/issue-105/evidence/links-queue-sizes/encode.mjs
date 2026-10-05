// Writes a store as LiNo for clink, and prints the size of the same links in
// links-queue's Binary Links Notation.
//
//   node encode.mjs <links-queue checkout> <points|doublets> <count>
import { writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const [, , checkout, shape, count] = process.argv;
const { BinaryNotation } = await import(
  pathToFileURL(resolve(checkout, 'js/src/protocol/binary-notation.js')).href
);

const links = [];
if (shape === 'points') {
  for (let i = 1; i <= count; i++) links.push({ id: i, source: i, target: i });
} else {
  // Doublets over earlier addresses, from a fixed linear congruential generator.
  let seed = 7;
  const below = (limit) => ((seed = (seed * 1103515245 + 12345) % 2147483648) % limit) + 1;
  links.push({ id: 1, source: 1, target: 1 });
  for (let i = 2; i <= count; i++) links.push({ id: i, source: below(i - 1), target: below(i - 1) });
}
writeFileSync(`${shape}-${count}.lino`, links.map(({ id, source, target }) => `(${id}: ${source} ${target})\n`).join(''));
console.log(BinaryNotation.encode(links).length);
