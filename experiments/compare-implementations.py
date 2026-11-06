#!/usr/bin/env python3
"""Compares tree, mp and selectors of two served GraphQL endpoints: host:port host:port."""

import json
import sys
import urllib.request

Q = {
    "tree": "link_id tree_id parent_id root_id depth self position_id",
    "mp": "item_id path_item_id path_item_depth root_id group_id position_id",
    "selectors": "item_id selector_id selector_include_id query_id",
}


def rows(host, table):
    body = json.dumps({"query": f"{{{table} {{{Q[table]}}}}}"}).encode()
    req = urllib.request.Request(
        f"http://{host}/v1/graphql", body, {"content-type": "application/json"}
    )
    return sorted(
        json.dumps(r, sort_keys=True)
        for r in json.load(urllib.request.urlopen(req))["data"][table]
    )


ok = True
for table in Q:
    a, b = rows(sys.argv[1], table), rows(sys.argv[2], table)
    print(table, len(a), len(b), "equal" if a == b else "DIFFERENT")
    ok &= a == b
sys.exit(0 if ok else 1)
