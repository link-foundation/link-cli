import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

/** A failed prerequisite must never turn the remaining skipped jobs green. */
export function checkPipeline(needs) {
  if (!needs || typeof needs !== 'object' || Array.isArray(needs) || !Object.keys(needs).length) {
    throw Error('Missing pipeline job results');
  }
  const failed = Object.entries(needs).filter(([, job]) => !['success', 'skipped'].includes(job?.result));
  if (failed.length) throw Error(`Pipeline jobs did not succeed: ${failed.map(([name, job]) => `${name} (${job?.result ?? 'missing'})`).join(', ')}`);
  if (!Object.values(needs).some((job) => job.result === 'success')) throw Error('Every pipeline job was skipped; no verification ran');
  return Object.entries(needs).filter(([, job]) => job.result === 'skipped').map(([name]) => name);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const skipped = checkPipeline(JSON.parse(process.env.NEEDS_JSON || '{}'));
    if (skipped.length) console.log(`::notice::Conditional jobs skipped: ${skipped.join(', ')}. See their event, path and prerequisite conditions above.`);
    console.log('All executed pipeline jobs succeeded.');
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
