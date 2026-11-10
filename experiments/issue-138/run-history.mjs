// Runs only the long-history isolation check on a wasm-pack build:
//   node experiments/issue-138/run-history.mjs js/pkg
import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { historyConformance } from "../../js/client/test/conformance/history.mjs";

const dir = resolve(process.argv[2] ?? "js/pkg");
const module = await import(pathToFileURL(join(dir, "clink_wasm.js")).href);
module.initSync({ module: readFileSync(join(dir, "clink_wasm_bg.wasm")) });
const started = Date.now();
await historyConformance(module);
console.log(`ok long random histories in ${Date.now() - started} ms`);
