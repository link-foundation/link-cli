import assert from 'node:assert/strict';
import test from 'node:test';
import { electedMember, waitForMembers } from './etcd-harness.mjs';

test('a replacement is required while the leader is paused, and any member may lead after healing', () => {
  const statuses = (members, leader) =>
    members.map((member_id) => ({ header: { member_id }, leader }));
  const majority = ['n0', 'n2'];
  assert.throws(() => electedMember(statuses(majority, 'n1')), /outside the sampled group/);
  assert.throws(
    () => electedMember([...statuses(['n0'], 'n0'), ...statuses(['n2'], 'n2')]),
    /disagree on leader/,
  );
  assert.equal(electedMember(statuses(majority, 'n2')), 1);
  assert.equal(electedMember(statuses(['n0', 'n1', 'n2'], 'n1')), 1);
});

test('a ready majority cannot hide a slow third member', async () => {
  const calls = new Map();
  let pauses = 0;
  const status = await waitForMembers(['http://n0', 'http://n1', 'http://n2'], {
    pause: async () => {
      pauses++;
    },
    fetch: async (url) => {
      const count = (calls.get(url) ?? 0) + 1;
      calls.set(url, count);
      if (url.includes('n1') && count === 1) throw Error('ECONNREFUSED');
      if (url.includes('n1') && count === 2) return new Response('{}', { status: 503 });
      if (url.includes('n1') && count === 3) return Response.json({ leader: '0' });
      return Response.json({ header: { member_id: url }, leader: 'elected' });
    },
  });
  assert.equal(status.length, 3);
  assert.equal(pauses, 3);
  assert.deepEqual([...calls.values()].sort(), [1, 1, 4]);
});
test('readiness has a finite attempt limit and preserves the failure', async () => {
  let calls = 0;
  await assert.rejects(
    waitForMembers(['http://absent'], {
      attempts: 2,
      pause: async () => {},
      fetch: async () => {
        calls++;
        throw Error('still absent');
      },
    }),
    /still absent/,
  );
  assert.equal(calls, 2);
});
