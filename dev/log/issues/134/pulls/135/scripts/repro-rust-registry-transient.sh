#!/usr/bin/env bash
# Reproduce the Rust release decision on a dropped crates.io connection.
#
# Usage: repro-rust-registry-transient.sh <git-revision> [always]
#
# Copies rust/scripts from <git-revision> into a temporary directory, points
# the crates.io and GitHub URLs at a local fake registry and runs
# check-release-needed.rs for link-cli 1.0.8, which is published and has its
# GitHub release. The first request for /api/v1/crates/link-cli/1.0.8 is
# closed without an answer; every later request succeeds. With `always`
# every request for it is closed, so the lookup never succeeds.
#
# Expected: should_release=false. Before the fix the script reads the dropped
# connection as "not published" and answers should_release=true with
# skip_bump=true, which republishes 1.0.8 and fails in publish-crate.rs.
# With `always` the fixed script must fail instead of guessing.
set -euo pipefail

revision="${1:?usage: $0 <git-revision> [always]}"
mode="${2:-once}"
repo="$(git rev-parse --show-toplevel)"
work="$(mktemp -d)"
trap 'kill "${server_pid:-0}" 2>/dev/null || true; rm -rf "$work"' EXIT

mkdir -p "$work/scripts" "$work/rust/src"
git -C "$repo" archive "$revision" rust/scripts | tar -x -C "$work/scripts" --strip-components=2
cat > "$work/rust/Cargo.toml" <<'TOML'
[package]
name = "link-cli"
version = "1.0.8"
edition = "2021"
TOML
touch "$work/rust/src/lib.rs"

cat > "$work/server.py" <<'PY'
import http.server, json, sys

dropped = []
always = sys.argv[1] == "always"

class Registry(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        print("GET", self.path, flush=True)
        if self.path == "/api/v1/crates/link-cli/1.0.8" and (always or not dropped):
            dropped.append(self.path)
            print("  -> connection dropped", flush=True)
            self.close_connection = True
            self.connection.close()
            return
        if self.path == "/api/v1/crates/link-cli":
            body = {"versions": [{"num": "1.0.8", "yanked": False}]}
        elif self.path == "/api/v1/crates/link-cli/1.0.8":
            body = {"version": {"num": "1.0.8"}}
        elif self.path == "/repos/link-foundation/link-cli/releases/tags/rust-v1.0.8":
            body = {"tag_name": "rust-v1.0.8"}
        else:
            self.send_response(404)
            self.end_headers()
            return
        data = json.dumps(body).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def log_message(self, *args):
        pass

server = http.server.HTTPServer(("127.0.0.1", 0), Registry)
print(server.server_address[1], flush=True)
server.serve_forever()
PY

python3 -I -u "$work/server.py" "$mode" > "$work/server.log" 2>&1 &
server_pid=$!
until [ -s "$work/server.log" ]; do sleep 0.1; done
port="$(head -1 "$work/server.log")"

sed -i "s#https://crates.io#http://127.0.0.1:$port#g; s#https://api.github.com#http://127.0.0.1:$port#g" \
  "$work"/scripts/*.rs

echo "== revision $(git -C "$repo" rev-parse --short "$revision"), connection dropped: $mode"
set +e
(cd "$work" && GITHUB_OUTPUT="$work/output" GITHUB_REPOSITORY=link-foundation/link-cli \
  rust-script scripts/check-release-needed.rs --rust-root rust --tag-prefix rust-v) > "$work/run.log" 2>&1
status=$?
set -e
grep -v '^ *\(Compiling\|Downloaded\|Downloading\|Updating\|Locking\|Adding\|Finished\|Blocking\)' "$work/run.log"
echo "exit=$status"
echo "== fake registry requests"
tail -n +2 "$work/server.log"
echo "== GITHUB_OUTPUT"
cat "$work/output" 2>/dev/null || echo "(not written)"
