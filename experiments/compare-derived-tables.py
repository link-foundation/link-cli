#!/usr/bin/env python3
"""Diff clink's derived `tree`/`selectors` with a reference Deep Postgres.

Usage: compare-derived-tables.py HOST:PORT MAX_ID  (container deep-postgres)
Only rows whose ids are all <= MAX_ID are compared, as the reference may hold more."""
import collections, json, subprocess, sys, urllib.request

url, max_id = f"http://{sys.argv[1]}/v1/graphql", int(sys.argv[2])
def gql(q):
    r = urllib.request.Request(url, json.dumps({"query": q}).encode())
    return json.load(urllib.request.urlopen(r))["data"]
def pg(q):
    o = subprocess.run(["docker","exec","deep-postgres","psql","-U","postgres","-tAF,","-c",q],capture_output=True,text=True).stdout
    return [tuple(int(x) if x else None for x in l.split(",")) for l in o.splitlines()]
def report(name, a, b, key):
    a, b = set(a), set(b)
    print(f"{name}: clink {len(a)} reference {len(b)} missing {len(b-a)} extra {len(a-b)}")
    for label, s in (("missing", b-a), ("extra", a-b)):
        c = collections.Counter(key(r) for r in s)
        print(" ", label, c.most_common(12), sorted(s)[:6])
ok = lambda r: all(x is None or x <= max_id for x in r)
t = gql("{tree{tree_id link_id parent_id depth root_id}}")["tree"]
report("tree(tree,link,parent)", [r for r in ((x["tree_id"],x["link_id"],x["parent_id"]) for x in t) if ok(r)],
  [r for r in pg("select tree_id,link_id,parent_id from tree") if ok(r)], lambda r:r[0])
report("tree+depth,root", [r for r in ((x["tree_id"],x["link_id"],x["parent_id"],x["depth"],x["root_id"]) for x in t) if ok(r)],
  [r for r in pg("select tree_id,link_id,parent_id,depth,root_id from tree") if ok(r)], lambda r:r[0])
s = gql("{selectors{selector_id item_id selector_include_id}}")["selectors"]
report("selectors", [r for r in ((x["selector_id"],x["item_id"],x["selector_include_id"]) for x in s) if ok(r)],
  [r for r in pg("select selector_id,item_id,selector_include_id from selectors") if ok(r)], lambda r:r[0])
