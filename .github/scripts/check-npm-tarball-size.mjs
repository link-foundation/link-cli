import { readFileSync } from 'node:fs';

// Match the JS pipeline template's compressed package budget. The budget
// leaves headroom under npm uploads known to have succeeded in that project.
const maxTarballBytes = 157 * 1024 * 1024;
const quote = (value) => JSON.stringify(value).replaceAll(':', '\\u003a');

try {
  const report = JSON.parse(readFileSync(0, 'utf8'));
  const pack = report?.[0];
  if (
    !Array.isArray(report) ||
    report.length !== 1 ||
    typeof pack?.name !== 'string' ||
    !Number.isSafeInteger(pack.size) ||
    pack.size < 1 ||
    !Array.isArray(pack.files) ||
    pack.files.length === 0 ||
    pack.files.some(
      (file) => typeof file?.path !== 'string' || !Number.isSafeInteger(file.size) || file.size < 0,
    )
  ) {
    throw new Error('Expected one npm pack report with a compressed size and file list');
  }
  if (pack.size > maxTarballBytes) {
    const largest = [...pack.files]
      .sort((first, second) => second.size - first.size)
      .slice(0, 10)
      .map((file) => `${quote(file.path)} (${file.size} bytes unpacked)`)
      .join(', ');
    throw new Error(
      `${quote(pack.name)} is ${pack.size} compressed bytes. Budget is ${maxTarballBytes} bytes. Largest files: ${largest}. Review the files list in package.json.`,
    );
  }
  console.log(`${quote(pack.name)} tarball: ${pack.size}/${maxTarballBytes} compressed bytes`);
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
