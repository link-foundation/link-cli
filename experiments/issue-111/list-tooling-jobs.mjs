// Lists jobs that install or run Rust or .NET tooling, with their needs/if.
import { readFileSync, readdirSync } from 'node:fs';
const tooling =
  /\b(?:cargo|rustup|rustc|wasm-pack|rust-script|dotnet)\b|dtolnay\/rust-toolchain|actions\/setup-dotnet|taiki-e\/install-action/;
for (const name of readdirSync('.github/workflows')) {
  const lines = readFileSync(`.github/workflows/${name}`, 'utf8').split('\n');
  const start = lines.indexOf('jobs:');
  let job = null;
  const jobs = [];
  for (const line of lines.slice(start + 1)) {
    if (/^[A-Za-z]/.test(line)) break;
    const h = /^ {2}([A-Za-z0-9_-]+):\s*$/.exec(line);
    if (h) {
      job = { name: h[1], body: [] };
      jobs.push(job);
      continue;
    }
    if (job) job.body.push(line);
  }
  for (const j of jobs) {
    const body = j.body.map((l) => l.replace(/(^|\s)#.*$/, '')).join('\n');
    const needs = /^ {4}needs:\s*(.*)$/m.exec(body)?.[1];
    const cond = /^ {4}if:\s*(.*)$/m.exec(body)?.[1];
    if (tooling.test(body))
      console.log(`${name} ${j.name} needs=${needs ?? '-'} if=${(cond ?? '-').slice(0, 60)}`);
  }
}
