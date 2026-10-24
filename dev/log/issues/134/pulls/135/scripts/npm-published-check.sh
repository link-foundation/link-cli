#!/usr/bin/env bash
# The "Check whether this version is published" logic of js-client.yml, old
# and new, for <package>@<version>. Set npm_config_registry to an unreachable
# URL (e.g. http://127.0.0.1:9) to see how each reads a registry failure.
# Usage: npm-published-check.sh <package> <version>
name="$1" version="$2"
echo "== old: if npm view ... >/dev/null 2>&1"
if npm view "$name@$version" version >/dev/null 2>&1; then echo "published=true"; else echo "published=false"; fi
echo "== new: only E404 means not published"
if output=$(npm view "$name@$version" version 2>&1); then
  echo "published=true"
elif grep -q 'code E404' <<< "$output"; then
  echo "published=false"
else
  echo "$output" | head -3
  echo "::error title=npm lookup failed::Could not ask npm whether $name@$version is published. How to fix: re-run the job."
  echo "exit 1"
fi
