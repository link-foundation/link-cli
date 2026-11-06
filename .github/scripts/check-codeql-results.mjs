#!/usr/bin/env node
// Analyze/upload success establishes delivery, not the absence of findings.
import { readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

function reportsIn(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const file = path.join(directory, entry.name);
    if (entry.isSymbolicLink()) throw new Error(`Unexpected SARIF symlink: ${file}`);
    if (entry.isDirectory()) return reportsIn(file);
    return entry.isFile() && entry.name.endsWith('.sarif') ? [file] : [];
  });
}

export function checkCodeqlResults(directory) {
  const reports = reportsIn(directory).sort();
  if (!reports.length) throw new Error(`No SARIF reports in ${directory}`);
  const findings = [];
  for (const file of reports) {
    const data = JSON.parse(readFileSync(file, 'utf8'));
    if (data?.version !== '2.1.0' || !Array.isArray(data.runs) || !data.runs.length) {
      throw new Error(`Invalid SARIF report: ${file}`);
    }
    for (const [index, run] of data.runs.entries()) {
      const context = `${file}, run ${index + 1}`;
      if (
        run?.tool?.driver?.name !== 'CodeQL' ||
        !Array.isArray(run.results) ||
        !Array.isArray(run.invocations) ||
        !run.invocations.length ||
        run.invocations.some((invocation) => invocation?.executionSuccessful !== true)
      ) {
        throw new Error(`Missing or unsuccessful CodeQL analysis: ${context}`);
      }
      for (const invocation of run.invocations) {
        for (const key of ['toolExecutionNotifications', 'toolConfigurationNotifications']) {
          const diagnostics = invocation[key] ?? [];
          if (!Array.isArray(diagnostics)) {
            throw new Error(`Malformed CodeQL diagnostics: ${context}, ${key}`);
          }
          for (const item of diagnostics) {
            if (!item || !['note', 'none'].includes(item.level)) {
              const location = item?.locations?.[0]?.physicalLocation;
              throw new Error(
                `CodeQL warning/error or malformed diagnostic: ${context}, ${key}: ` +
                  `${item?.descriptor?.id ?? '?'} ${location?.artifactLocation?.uri ?? '?'}:` +
                  `${location?.region?.startLine ?? '?'}: ${item?.message?.text ?? 'missing diagnostic message'}`,
              );
            }
          }
        }
      }
      for (const result of run.results) {
        if (
          !result ||
          typeof result.ruleId !== 'string' ||
          !(
            typeof result.message?.text === 'string' || typeof result.message?.markdown === 'string'
          )
        ) {
          throw new Error(`Malformed SARIF result: ${context}`);
        }
        if (result.baselineState === 'absent') continue;
        const location = result.locations?.[0]?.physicalLocation;
        findings.push(
          `${context}: ${result.ruleId} (${result.level ?? 'warning'}) ` +
            `${location?.artifactLocation?.uri ?? '?'}:${location?.region?.startLine ?? '?'}: ` +
            `${result.message.text ?? result.message.markdown}`,
        );
      }
    }
  }
  if (findings.length)
    throw new Error(`${findings.length} unresolved CodeQL finding(s):\n${findings.join('\n')}`);
  return reports.length;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1])) {
  try {
    console.log(
      `CodeQL gate: ${checkCodeqlResults(process.argv[2] ?? 'codeql-results')} report(s), no unresolved findings`,
    );
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
