#!/usr/bin/env bash
# A served store with permissions and a token table stored as links (issue #149).
# Usage: serve-with-tokens.sh <clink command...>
set -euo pipefail
dir=$(mktemp -d)
db="$dir/data.links"
tokens="$dir/tokens.links"
clink=("$@")
server=
cleanup() {
  if [ -n "$server" ]; then kill "$server" 2>/dev/null || true; wait "$server" 2>/dev/null || true; fi
  rm -rf "$dir"
}
trap cleanup EXIT

"${clink[@]}" --db "$db" --auto-create-missing-references \
  '() ((Type: Type Type) (Extends: Extends Extends) (AllowRead: AllowRead AllowRead) (AllowWrite: AllowWrite AllowWrite) (Anything: Anything Anything) (User: Type User) (Doc: Type Doc) (Public: Type Public) (Note: Doc Note) (Secret: Doc Secret) (alice: User alice) (note: Note note) (secret: Secret secret) (news: Public news) (AllowWrite (User Note)) (AllowRead (User Public)))' >/dev/null

echo '# a token of alice, kept as links in a file of its own'
"${clink[@]}" --db "$db" --tokens "$tokens" --subject alice --grant-token alice-secret

"${clink[@]}" --db "$db" --serve 127.0.0.1:0 --permissions self --tokens "$tokens" > "$dir/banner" &
server=$!
for _ in $(seq 100); do [ -s "$dir/banner" ] && break; sleep 0.1; done
address=$(sed 's/^clink server listening on //' "$dir/banner")

echo '# a guest connection'
"${clink[@]}" --connect "$address" '(count: ())'
echo '# alice, after (auth: alice-secret)'
"${clink[@]}" --connect "$address" --token alice-secret '(count: ())'
"${clink[@]}" --connect "$address" --token alice-secret
echo '# a wrong token'
"${clink[@]}" --connect "$address" --token wrong '(count: ())' 2>&1 || true
