#!/usr/bin/env bash
# Builds the type hierarchy of issue #148 with plain clink queries.
# Usage: run.sh <clink command...>
set -euo pipefail
dir=$(mktemp -d)
db="$dir/types.links"
clink=("$@")
echo '# define'
"${clink[@]}" --db "$db" --auto-create-missing-references \
  '() ((Type: Type Type) (Extends: Extends Extends) (User: Type User) (Doc: Type Doc) (Public: Type Public) (Note: Doc Note) (News: Note News) (Extends (News Public)) (today: News today))' --after
echo '# types'
"${clink[@]}" --db "$db" '((($t: $p $t)) (($t: $p $t)))' --changes
echo '# values of News'
"${clink[@]}" --db "$db" '((($v: News $x)) (($v: News $x)))' --changes
echo '# parents marked by Extends'
"${clink[@]}" --db "$db" '((($m: Extends $p)) (($m: Extends $p)))' --changes
rm -rf "$dir"
