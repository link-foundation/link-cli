import { describe, expect, test } from 'bun:test';
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

describe('checkout Git configuration policy', () => {
  test('configures init.defaultBranch before checkout in every workflow', () => {
    const findings = [];
    for (const file of readdirSync('.github/workflows').filter((name) => /\.ya?ml$/.test(name))) {
      const workflow = Bun.YAML.parse(readFileSync(join('.github/workflows', file), 'utf-8'));
      const checksOut = Object.values(workflow.jobs).some((job) =>
        job.steps?.some((step) => step.uses?.startsWith('actions/checkout@'))
      );
      if (!checksOut) continue;
      const env = workflow.env ?? {};
      if (env.GIT_CONFIG_COUNT !== '1' || env.GIT_CONFIG_KEY_0 !== 'init.defaultBranch' ||
          env.GIT_CONFIG_VALUE_0 !== 'main') {
        findings.push(file);
      }
      for (const [name, job] of Object.entries(workflow.jobs)) {
        for (const step of job.steps ?? []) {
          if (!step.uses?.startsWith('actions/checkout@')) continue;
          const effective = { ...env, ...job.env, ...step.env };
          expect(effective, `${file}: ${name} checkout overrides Git config`).toMatchObject({
            GIT_CONFIG_COUNT: '1', GIT_CONFIG_KEY_0: 'init.defaultBranch', GIT_CONFIG_VALUE_0: 'main',
          });
        }
      }
    }
    expect(findings, `Missing checkout configuration: ${findings.join(', ')}`).toEqual([]);
  });
});
