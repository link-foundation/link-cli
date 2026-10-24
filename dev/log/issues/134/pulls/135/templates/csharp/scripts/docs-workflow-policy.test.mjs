import { describe, expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';

const DOCS_WORKFLOW = '.github/workflows/docs.yml';
const PAGES_DEPLOYMENT_TRIGGER =
  "(github.event_name == 'push' && github.ref == 'refs/heads/main') || github.event_name == 'workflow_dispatch'";
const PAGES_DEPLOYMENT_OPT_IN = "vars.DEPLOY_GITHUB_PAGES == 'true'";
const PAGES_DEPLOYMENT_DISABLED = "vars.DEPLOY_GITHUB_PAGES != 'true'";

function readWorkflow(filePath) {
  return readFileSync(filePath, 'utf-8').replaceAll('\r\n', '\n');
}

function getJobBlocks(workflow) {
  const lines = workflow.split('\n');
  const jobsStart = lines.findIndex((line) => line === 'jobs:');
  if (jobsStart === -1) {
    return new Map();
  }

  const blocks = new Map();
  let currentName = '';
  let currentLines = [];

  for (const line of lines.slice(jobsStart + 1)) {
    const match = /^  ([a-zA-Z0-9_-]+):\s*$/.exec(line);
    if (match) {
      if (currentName) {
        blocks.set(currentName, currentLines.join('\n'));
      }
      currentName = match[1];
      currentLines = [line];
      continue;
    }

    if (currentName) {
      currentLines.push(line);
    }
  }

  if (currentName) {
    blocks.set(currentName, currentLines.join('\n'));
  }

  return blocks;
}

function getJobCondition(jobBlock) {
  const lines = jobBlock.split('\n');
  const ifIndex = lines.findIndex((line) => /^    if:/.test(line));
  if (ifIndex === -1) {
    return '';
  }

  const conditionLines = [lines[ifIndex].replace(/^    if:\s*\|?\s*/, '')];
  for (const line of lines.slice(ifIndex + 1)) {
    if (/^ {6,}\S/.test(line) || line.trim() === '') {
      conditionLines.push(line);
      continue;
    }
    break;
  }

  return conditionLines.join('\n').trim();
}

function getStepBlock(jobBlock, stepName) {
  const lines = jobBlock.split('\n');
  const stepStart = lines.findIndex(
    (line) => line.trim() === `- name: ${stepName}`
  );
  if (stepStart === -1) {
    return '';
  }

  const stepLines = [lines[stepStart]];
  for (const line of lines.slice(stepStart + 1)) {
    if (/^ {6}- /.test(line)) {
      break;
    }
    stepLines.push(line);
  }

  return stepLines.join('\n');
}

describe('docs workflow policy', () => {
  test('keeps documentation builds independent from Pages deployment opt-in', () => {
    const workflow = readWorkflow(DOCS_WORKFLOW);
    const buildJob = getJobBlocks(workflow).get('build');
    const buildStep = getStepBlock(buildJob, 'Build documentation site');

    expect(buildStep).toContain('docfx docfx.json -o _site');
    expect(buildStep).not.toContain('DEPLOY_GITHUB_PAGES');
    expect(buildStep).not.toContain(PAGES_DEPLOYMENT_OPT_IN);
  });

  test('gates GitHub Pages publishing behind DEPLOY_GITHUB_PAGES=true', () => {
    const workflow = readWorkflow(DOCS_WORKFLOW);
    const jobBlocks = getJobBlocks(workflow);
    const buildJob = jobBlocks.get('build');
    const deployJob = jobBlocks.get('deploy');

    for (const stepName of [
      'Configure GitHub Pages',
      'Upload GitHub Pages artifact',
    ]) {
      const step = getStepBlock(buildJob, stepName);

      expect(step).toContain(PAGES_DEPLOYMENT_TRIGGER);
      expect(step).toContain(PAGES_DEPLOYMENT_OPT_IN);
    }

    const deployCondition = getJobCondition(deployJob);
    expect(deployCondition).toContain(PAGES_DEPLOYMENT_TRIGGER);
    expect(deployCondition).toContain(PAGES_DEPLOYMENT_OPT_IN);
  });

  test('explains skipped GitHub Pages publishing when repository opt-in is absent', () => {
    const workflow = readWorkflow(DOCS_WORKFLOW);
    const buildJob = getJobBlocks(workflow).get('build');
    const skipStep = getStepBlock(buildJob, 'Skip GitHub Pages deployment');

    expect(skipStep).toContain(PAGES_DEPLOYMENT_TRIGGER);
    expect(skipStep).toContain(PAGES_DEPLOYMENT_DISABLED);
    expect(skipStep).toContain('DEPLOY_GITHUB_PAGES');
    expect(skipStep).toContain('::notice::');
  });
});
