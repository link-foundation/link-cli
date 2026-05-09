import assert from 'node:assert/strict';
import test from 'node:test';
import { buildGraph } from '../src/linkGraph.js';

test('buildGraph reflects create, delete, and recreate snapshots without stale nodes', () => {
  const created = buildGraph([
    { id: 1, source: 1, target: 1, name: 'Type' },
    { id: 2, source: 1, target: 2, name: 'Child' },
  ]);

  assert.deepEqual(created.nodes.map((node) => node.id), [1, 2]);
  assert.equal(created.points.get(1).label, 'Type');
  assert.equal(created.points.get(2).label, 'Child');

  const deleted = buildGraph([{ id: 1, source: 1, target: 1, name: 'Type' }]);
  assert.deepEqual(deleted.nodes.map((node) => node.id), [1]);
  assert.equal(deleted.edges.length, 2);

  const recreated = buildGraph([
    { id: 1, source: 1, target: 1, name: 'Type' },
    { id: 2, source: 2, target: 2, name: 'Child' },
  ]);

  assert.deepEqual(recreated.nodes.map((node) => node.id), [1, 2]);
  assert.equal(recreated.points.get(2).label, 'Child');
  assert.equal(recreated.edges.filter((edge) => edge.from === 2 || edge.to === 2).length, 2);
});
