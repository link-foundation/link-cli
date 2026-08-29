#!/usr/bin/env bash
# Proves the FileShare-based lock is honoured between separate processes.
set -u
cd "$(dirname "$0")"
lock="$(mktemp -d)/db.links.lock"
dotnet build -v q --nologo >/dev/null || exit 1
app="bin/Debug/net10.0/csharp-file-lock.dll"

check() { # holder-mode challenger-mode expected
  local out
  out="$(mktemp)"
  # The holder is started as the app itself rather than through
  # `dotnet run` so that its PID can be signalled directly, and it holds
  # far longer than the challenger needs so the window never closes early.
  dotnet "$app" hold "$lock" "$1" 60000 >"$out" &
  holder=$!
  # Wait for the holder to report that it *owns* the lock instead of
  # guessing at a startup delay; a fixed sleep is racy under CI load.
  for _ in $(seq 1 300); do
    grep -q held "$out" && break
    sleep 0.1
  done
  if ! grep -q held "$out"; then
    echo "FAIL $1 vs $2 -> holder never acquired the lock"
    kill "$holder" 2>/dev/null
    exit 1
  fi
  actual="$(dotnet "$app" try "$lock" "$2")"
  # The holder must still be alive, otherwise the challenger raced an
  # already-released lock and the result proves nothing.
  if ! kill -0 "$holder" 2>/dev/null; then
    echo "FAIL $1 vs $2 -> holder exited before the challenger ran"
    exit 1
  fi
  kill "$holder" 2>/dev/null
  wait "$holder" 2>/dev/null
  if [ "$actual" = "$3" ]; then echo "ok   $1 vs $2 -> $actual"; else echo "FAIL $1 vs $2 -> $actual (expected $3)"; exit 1; fi
}

check exclusive exclusive blocked
check exclusive shared    blocked
check shared    shared    acquired
echo "all cross-process lock expectations hold"
