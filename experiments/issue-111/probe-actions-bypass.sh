#!/usr/bin/env bash
# Issue #111: can GitHub Actions (app 15368, the identity of GITHUB_TOKEN) be a
# bypass actor of a repository ruleset? The release jobs push version commits
# to main, so the rules requiring the stage checks need a bypass for them.
#
# The probe creates a disabled ruleset on an unused ref and deletes it again if
# GitHub accepts it. On 2026-11-14 GitHub answered HTTP 422 "Actor GitHub
# Actions integration must be part of the ruleset source or owner
# organization", so the release jobs push with a deploy key instead.
# Usage: probe-actions-bypass.sh [owner/repo]   (needs repository admin)
set -euo pipefail
repo=${1:-link-foundation/link-cli}
if id=$(gh api -X POST "repos/$repo/rulesets" --jq .id --input - <<'JSON'
{
  "name": "issue-111 probe: GitHub Actions bypass",
  "target": "branch",
  "enforcement": "disabled",
  "conditions": {"ref_name": {"include": ["refs/heads/issue-111-probe"], "exclude": []}},
  "bypass_actors": [{"actor_id": 15368, "actor_type": "Integration", "bypass_mode": "always"}],
  "rules": [{"type": "non_fast_forward"}]
}
JSON
); then
  echo "accepted: GitHub Actions can be a bypass actor now (ruleset $id); deleting the probe"
  gh api -X DELETE "repos/$repo/rulesets/$id"
else
  echo "rejected: GitHub Actions cannot be a bypass actor; keep the deploy key"
fi
