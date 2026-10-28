// Finite protocol-boundary probe: canonical documents retain text, not kinds.
import assert from 'node:assert/strict';
import { formatDocument, parseDocument } from '../../js/client/src/lino.js';

for (const [literal, bare] of [
  ["'4294967312'", '4294967312'],
  ["'18446744073709551615'", '18446744073709551615'],
  ["'*'", '*'],
  ["'$x'", '$x'],
  ['~1{34323934393637333132}', '4294967312'],
]) {
  const quotedQuery = `() ((${literal}: ${literal} ${literal}))`;
  const bareQuery = `() ((${bare}: ${bare} ${bare}))`;
  const quoted = parseDocument(quotedQuery);
  const unquoted = parseDocument(bareQuery);
  assert.deepEqual(quoted, unquoted);
  assert.equal(formatDocument(quoted), formatDocument(unquoted));
  console.log(JSON.stringify({ original: quotedQuery, canonical: formatDocument(quoted), kindsLost: true }));
}
