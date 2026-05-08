#!/usr/bin/env node

import { readFileSync } from 'fs';

const workflowPath = '.github/workflows/wasm.yml';
const workflow = readFileSync(workflowPath, 'utf8');

function fail(message) {
  console.error(`FAIL: ${message}`);
  process.exitCode = 1;
}

function pass(message) {
  console.log(`PASS: ${message}`);
}

if (workflow.includes("github.event_name == 'push' && github.ref == 'refs/heads/main'")) {
  fail('GitHub Pages deployment still runs automatically on main pushes');
} else {
  pass('GitHub Pages deployment is not automatic on main pushes');
}

if (!workflow.includes('deploy_pages:')) {
  fail('workflow_dispatch deploy_pages input is missing');
} else {
  pass('workflow_dispatch deploy_pages input exists');
}

if (!workflow.includes("github.event_name == 'workflow_dispatch'") || !workflow.includes('inputs.deploy_pages')) {
  fail('Deploy job is not gated by manual workflow_dispatch opt-in');
} else {
  pass('Deploy job is gated by manual workflow_dispatch opt-in');
}

if (!workflow.includes('actions/configure-pages@v5')) {
  fail('GitHub Pages configuration step is missing');
} else {
  pass('GitHub Pages configuration step is still present for opt-in deployments');
}
