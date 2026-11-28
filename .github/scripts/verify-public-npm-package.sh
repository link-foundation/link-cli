#!/usr/bin/env bash
set -euo pipefail

package_spec=$(node -p 'const {name, version} = require("./package.json"); name + "@" + version')
scratch=$(mktemp -d)
trap 'rm -rf -- "$scratch"' EXIT
cd "$scratch"
touch npmrc
unset NODE_AUTH_TOKEN NPM_TOKEN

# Use a clean cache and config so a consumer can obtain the exact tarball.
# npm retries transient HTTP errors. Retry a new version's 404 briefly while
# the public registry propagates it.
for attempt in 1 2 3; do
  if npm pack "$package_spec" --ignore-scripts --registry=https://registry.npmjs.org \
    --userconfig=/dev/null --globalconfig="$scratch/npmrc" --cache="$scratch/cache" \
    --prefer-online --fetch-timeout=15000 --fetch-retries=1 >pack.log 2>&1; then
    cat pack.log
    printf 'Downloaded %s anonymously from npm.\n' "$package_spec"
    exit 0
  fi
  if [[ "$attempt" == 3 ]] || ! grep -q 'code E404' pack.log; then
    cat pack.log >&2
    printf 'Could not download %s anonymously from npm. Check public visibility and retry verification.\n' "$package_spec" >&2
    exit 1
  fi
  sleep "$((attempt * 5))"
done
