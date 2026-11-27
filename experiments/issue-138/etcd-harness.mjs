export async function memberStatus(endpoint, fetch = globalThis.fetch) {
  const response = await fetch(`${endpoint}/v3/maintenance/status`, {
    method: 'POST',
    body: '{}',
    signal: AbortSignal.timeout(2000),
  });
  if (!response.ok) throw Error(`member status HTTP ${response.status}`);
  const status = await response.json();
  if (!status.header?.member_id || !status.leader || status.leader === '0') {
    throw Error('member has no leader');
  }
  return status;
}

// Require agreement and a leader in the sampled group. A majority that still
// reports the paused member has not elected its replacement yet.
export function electedMember(statuses) {
  const leader = statuses[0]?.leader;
  if (!leader || statuses.some((status) => status.leader !== leader)) {
    throw Error(
      `members disagree on leader: ${statuses.map((status) => status.leader).join(', ')}`,
    );
  }
  const index = statuses.findIndex((status) => status.header.member_id === leader);
  if (index < 0) throw Error(`reported leader ${leader} is outside the sampled group`);
  return index;
}

// Each intended member must be ready before injecting failures. A majority's
// readiness alone says nothing about a slower member's listening socket.
export async function waitForMembers(
  endpoints,
  {
    fetch = globalThis.fetch,
    attempts = 30,
    pause = (ms) => new Promise((done) => setTimeout(done, ms)),
  } = {},
) {
  return Promise.all(
    endpoints.map(async (endpoint) => {
      let failure;
      for (let attempt = 0; attempt < attempts; attempt++) {
        try {
          return await memberStatus(endpoint, fetch);
        } catch (error) {
          failure = error;
          await pause(100);
        }
      }
      throw failure;
    }),
  );
}
