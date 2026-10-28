import { readFileSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { causalSessionConformance } from '../../js/client/test/conformance/causal-session.mjs';
const directory = resolve(process.env.CLINK_WASM ?? 'experiments/issue-138/native-lsm-wasm-package');
const binding = await import(pathToFileURL(join(directory, 'clink_wasm.js')).href);
binding.initSync({ module: readFileSync(join(directory, 'clink_wasm_bg.wasm')) });
await causalSessionConformance(binding);
