#!/usr/bin/env bash
# Reproduce check-web-archive.mjs reading a rate-limited Wayback API as
# "no archived version".
#
# Usage: repro-web-archive-429.sh <path/to/check-web-archive.mjs> [always]
#
# Copies the script into a temporary directory, points its Wayback API at a
# local fake that answers the first request with HTTP 429 (every request with
# `always`) and has a snapshot for every URL otherwise, then runs it on a
# lychee report with one broken link.
#
# Expected: the link is reported with its archive.org snapshot. Before the fix
# the script asks once, reads the 429 as "not archived" and fails the job with
# "No archived version was found in the Wayback Machine". With `always` the
# fixed script must say that the lookup failed, not that there is no snapshot.
set -euo pipefail

script="$(realpath "${1:?usage: $0 <check-web-archive.mjs> [always]}")"
mode="${2:-once}"
work="$(mktemp -d)"
trap 'kill "${server_pid:-0}" 2>/dev/null || true; rm -rf "$work"' EXIT

cp "$script" "$work/check-web-archive.mjs"
cat > "$work/out.md" <<'MD'
# Summary

## Errors per input

### Errors in README.md

* [404] <https://example.com/moved-page> | Rejected status code (this depends on your "accept" configuration): Not Found
MD

cat > "$work/server.py" <<'PY'
import http.server, json, sys, urllib.parse

always = sys.argv[1] == "always"
served = []

class Wayback(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        print("GET", self.path, flush=True)
        served.append(self.path)
        if always or len(served) == 1:
            print("  -> 429", flush=True)
            data = b"<html><body><h1>429 Too Many Requests</h1></body></html>"
            self.send_response(429)
            self.send_header("Content-Type", "text/html")
        else:
            url = urllib.parse.parse_qs(urllib.parse.urlparse(self.path).query)["url"][0]
            snapshot = {"available": True, "status": "200", "timestamp": "20240101000000",
                        "url": "http://web.archive.org/web/20240101000000/" + url}
            data = json.dumps({"url": url, "archived_snapshots": {"closest": snapshot}}).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def log_message(self, *args):
        pass

server = http.server.HTTPServer(("127.0.0.1", 0), Wayback)
print(server.server_address[1], flush=True)
server.serve_forever()
PY

python3 -I -u "$work/server.py" "$mode" > "$work/server.log" 2>&1 &
server_pid=$!
until [ -s "$work/server.log" ]; do sleep 0.1; done
port="$(head -1 "$work/server.log")"
sed -i "s#https://archive.org/wayback/available#http://127.0.0.1:$port/wayback/available#" "$work/check-web-archive.mjs"

echo "== $1, Wayback API rate-limited: $mode"
set +e
(cd "$work" && GITHUB_OUTPUT="$work/output" LYCHEE_OUTPUT="$work/out.md" \
  RECOVERED_URLS="$work/none.txt" node check-web-archive.mjs) > "$work/run.log" 2>&1
status=$?
set -e
cat "$work/run.log"
echo "exit=$status"
echo "== fake Wayback API requests"
tail -n +2 "$work/server.log"
echo "== GITHUB_OUTPUT"
cat "$work/output" 2>/dev/null || echo "(not written)"
