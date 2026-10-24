// Each intended member must be ready before injecting failures. A majority's
// readiness alone says nothing about a slower member's listening socket.
export async function waitForMembers(endpoints, { fetch = globalThis.fetch, attempts = 30, pause = (ms) => new Promise((done) => setTimeout(done, ms)) } = {}) {
  return Promise.all(endpoints.map(async (endpoint) => {
    let failure;
    for (let attempt = 0; attempt < attempts; attempt++) {
      try {
        const response = await fetch(`${endpoint}/v3/maintenance/status`, { method: 'POST', body: '{}', signal: AbortSignal.timeout(2000) });
        if (!response.ok) throw Error(`member status HTTP ${response.status}`);
        const status = await response.json();
        if (!status.header?.member_id || !status.leader || status.leader === '0') throw Error('member has no leader');
        return status;
      } catch (error) { failure = error; await pause(100); }
    }
    throw failure;
  }));
}
