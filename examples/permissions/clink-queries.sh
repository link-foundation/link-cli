#!/usr/bin/env bash
# Permissions stored as links (issue #149), with plain clink queries.
# Usage: clink-queries.sh <clink command...>
set -euo pipefail
dir=$(mktemp -d)
trap 'rm -rf "$dir"' EXIT
db="$dir/data.links"
clink=("$@")

echo '# types, values and rules, written without --permissions'
"${clink[@]}" --db "$db" --auto-create-missing-references \
  '() ((Type: Type Type) (Extends: Extends Extends) (AllowRead: AllowRead AllowRead) (AllowWrite: AllowWrite AllowWrite) (AllowReference: AllowReference AllowReference) (Anything: Anything Anything) (User: Type User) (Admin: User Admin) (Doc: Type Doc) (Public: Type Public) (Note: Doc Note) (Secret: Doc Secret) (alice: User alice) (boss: Admin boss) (note: Note note) (secret: Secret secret) (news: Public news) (AllowWrite (Admin Anything)) (AllowWrite (User Note)) (AllowRead (User Public)))'
"${clink[@]}" --db "$db" --after

echo '# what alice sees: notes (write implies read) and public links'
"${clink[@]}" --db "$db" --permissions self --subject alice --after

echo '# a guest sees nothing'
"${clink[@]}" --db "$db" --permissions self --after

echo '# alice may not turn her note into a public link'
"${clink[@]}" --db "$db" --permissions self --subject alice '((note: Note note)) ((note: Public note))' || echo "exit code $?"

echo '# a secret is as missing to alice as a link that never was: its name is unknown'
"${clink[@]}" --db "$db" --permissions self --subject alice '((secret: Secret secret)) ()' || echo "exit code $?"
"${clink[@]}" --db "$db" --permissions self --subject alice '() ((Note secret))' || echo "exit code $?"
"${clink[@]}" --db "$db" --permissions self --subject alice '() ((Note nothing))' || echo "exit code $?"

echo '# the boss writes anything'
"${clink[@]}" --db "$db" --permissions self --subject boss --changes '((news: Public news)) ((news: Note news))'

echo '# the store is unchanged but for the boss'"'"'s update'
"${clink[@]}" --db "$db" --after

echo '# a rule lets alice refer to the secrets'
"${clink[@]}" --db "$db" --auto-create-missing-references '() ((AllowReference (User Secret)))' >/dev/null
"${clink[@]}" --db "$db" --permissions self --subject alice --changes '() ((Note secret))'

echo '# she still may not read them'
"${clink[@]}" --db "$db" --permissions self --subject alice --after
