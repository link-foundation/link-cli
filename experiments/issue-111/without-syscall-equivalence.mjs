// CodeQL js/polynomial-redos (issue #111): checks that withoutSyscall in
// js/server/src/error-report.js removes exactly what the regular expression
// it replaced removed, on every string up to 7 characters over an
// adversarial alphabet, and that it is linear. Run: node experiments/issue-111/without-syscall-equivalence.mjs
import { withoutSyscall } from '../../js/server/src/error-report.js';
const old = (t) => t.replace(/, [a-z]+(?: '.*')?$/, '');
const alphabet = [',', ' ', 'a', 'b', "'", '\n', 'X', ':'];
let n = 0;
function gen(prefix, depth) {
  if (old(prefix) !== withoutSyscall(prefix)) {
    console.log(
      'DIFF',
      JSON.stringify(prefix),
      JSON.stringify(old(prefix)),
      JSON.stringify(withoutSyscall(prefix)),
    );
    process.exit(1);
  }
  n++;
  if (depth === 0) return;
  for (const c of alphabet) gen(prefix + c, depth - 1);
}
gen('', 7);
for (const t of [
  "ENOENT: no such file or directory, open 'a, b'",
  'x, open',
  "EACCES: permission denied, mkdir '/x/y'",
])
  if (old(t) !== withoutSyscall(t)) throw t;
const big = ", a '".repeat(20000);
let t0 = Date.now();
withoutSyscall(big);
console.log('equal on', n, 'strings; 100k-char adversarial input in', Date.now() - t0, 'ms');
