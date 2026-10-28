// Frozen incorrect ordering: removing deletion markers before policy selection.
import assert from 'node:assert/strict';
import { CausalVersion } from '../../js/client/src/engine-causal-version.js';
const author = value => value.toString(16).padStart(32, '0');
const live = CausalVersion.advance('1', author(1), '0', [], { source: '0', target: '2', name: 'live' });
const deletion = CausalVersion.advance('1', author(2), '0', [], null);
const wronglyVisible = CausalVersion.merge([live, deletion].filter(value => value.payload !== null), []);
assert.deepEqual(wronglyVisible, [], 'filtering deletion markers before maximum-dot selection resurrects the losing row');
