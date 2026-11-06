import { resolve } from 'node:path';

const object = (x) => x !== null && typeof x === 'object' && !Array.isArray(x);
const array = (x, name, nonempty = false) => {
  if (!Array.isArray(x) || (nonempty && !x.length)) throw Error(`Missing/malformed ${name}`);
  return x;
};
function projects(data, expected, kind) {
  if (data?.version !== 1 || data.problems?.length || data.errors?.length)
    throw Error(`Unverified ${kind} provider/schema`);
  const found = new Map();
  for (const project of array(data.projects, `${kind} projects`, true)) {
    if (typeof project?.path !== 'string' || found.has(resolve(project.path)))
      throw Error(`Invalid ${kind} project`);
    // dotnet 8 emits exactly {path} for a successfully audited clean project.
    // The independently captured complete graph still supplies every TFM.
    if (kind === 'audit' && project.frameworks === undefined && Object.keys(project).length === 1) {
      found.set(resolve(project.path), null);
      continue;
    }
    const frameworks = new Map();
    for (const framework of array(project.frameworks, `${kind} frameworks`, true)) {
      if (!framework?.framework || frameworks.has(framework.framework))
        throw Error('Invalid/duplicate framework');
      frameworks.set(framework.framework, framework);
    }
    found.set(resolve(project.path), frameworks);
  }
  if (found.size !== expected.length || expected.some((p) => !found.has(resolve(p))))
    throw Error(`Missing/unexpected ${kind} project graph`);
  return found;
}

export function validateAudit(audit, graph, expected) {
  array(expected, 'expected projects', true);
  const resolved = projects(graph, expected, 'dependency graph');
  const audited = projects(audit, expected, 'audit');
  for (const [project, frameworks] of resolved) {
    const checked = audited.get(project);
    if (checked !== null && checked.size !== frameworks.size)
      throw Error('Audit omitted resolved framework');
    for (const [name, framework] of frameworks) {
      const report = checked === null ? {} : checked.get(name);
      if (!report) throw Error('Audit omitted resolved framework');
      for (const kind of ['topLevelPackages', 'transitivePackages']) {
        for (const pkg of array(framework[kind] ?? [], `resolved ${kind}`)) {
          if (
            typeof pkg?.id !== 'string' ||
            !pkg.id ||
            typeof pkg.resolvedVersion !== 'string' ||
            !pkg.resolvedVersion
          )
            throw Error('Unresolved dependency coordinate');
        }
        for (const pkg of array(report[kind] ?? [], `audit ${kind}`)) {
          if (!object(pkg) || !Array.isArray(pkg.vulnerabilities) || !pkg.vulnerabilities.length)
            throw Error('Malformed vulnerability verdict');
          throw Error(`Vulnerable ${kind}: ${pkg.id}@${pkg.resolvedVersion}`);
        }
      }
    }
  }
}
