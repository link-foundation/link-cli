#!/usr/bin/env bash
# Run the #[cfg(test)] suites of every rust-script helper in rust/scripts.
#
# The release scripts commit and push straight to main, where nothing
# re-checks their output, so their own tests are the only gate (issue #134).
set -euo pipefail

cd "$(dirname "$0")"
status=0
for script in *.rs; do
  # Modules included by other scripts (rust-paths.rs) are tested through them.
  grep -q '^#\[cfg(test)\]' "$script" || continue
  echo "::group::rust-script --test $script"
  if ! rust-script --test "$script"; then
    echo "::error file=rust/scripts/$script::rust-script --test $script failed"
    status=1
  fi
  echo "::endgroup::"
done
exit "$status"
