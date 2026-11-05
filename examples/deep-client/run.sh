#!/usr/bin/env bash
# The acceptance scenario of issue #153: the real DeepClient against
# `clink --serve --graphql`. Dependencies go to a temporary folder.
# Usage: run.sh <clink command...>
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
dir=$(mktemp -d)
log="$dir/server.log"
server=
cleanup() {
  [ -n "$server" ] && kill "$server" 2>/dev/null || true
  rm -rf "$dir"
}
trap cleanup EXIT

gunzip -c "$root/docs/protocol/graphql/deep-fresh.json.gz" >"$dir/fresh.json"
"$@" --db "$dir/deep.links" --serve 127.0.0.1:0 --graphql --jwt-secret example \
  --deep-package "$dir/fresh.json" >"$log" 2>&1 &
server=$!
address=
for _ in $(seq 1 300); do
  address=$(sed -n 's/.*listening on //p' "$log" | head -n 1)
  [ -n "$address" ] && break
  kill -0 "$server" 2>/dev/null || { cat "$log"; exit 1; }
  sleep 0.2
done
[ -n "$address" ] || { echo "the server did not start"; cat "$log"; exit 1; }

cd "$dir"
npm init -y >/dev/null
npm install --silent --no-audit --no-fund --legacy-peer-deps --ignore-scripts \
  @deep-foundation/deeplinks@0.0.620 @deep-foundation/materialized-path@0.0.35 \
  @deep-foundation/react-hasura@0.0.29 @deep-foundation/store@0.0.32 \
  @apollo/client@3 @apollo/react-hooks@4 apollo-boost@0.4 apollo-link@1 \
  apollo-link-ws@1 graphql@15 graphql-tag@2 react@18 react-dom@18 \
  react-cookie@8 subscriptions-transport-ws@0.11
cp "$here/deep-client.cjs" .
node deep-client.cjs "$address/v1/graphql"
