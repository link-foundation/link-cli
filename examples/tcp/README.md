# LiNo over TCP — examples

Runnable demonstrations of `clink --serve` and `clink --connect`, added for
issue [#105](https://github.com/link-foundation/link-cli/issues/105).

## Scripts

| File | What it shows |
|------|---------------|
| `run-interop.sh` | One port serves a database and the other port queries it, over the text protocol and every binary option |
| `README.md` | This file |

```bash
./examples/tcp/run-interop.sh          # Rust server, C# client
./examples/tcp/run-interop.sh csharp   # C# server, Rust client
```

Both directions print the same thing:

```
server: clink server listening on 127.0.0.1:40123
[text] () ((1: 1 1))
[--protocol binary] () ((2: 2 2))
[--external-references] () ((3: 3 3))
[--sequences] () ((4: 4 4))
[--progressive-widths] () ((5: 5 5))
[--external-references --sequences --progressive-widths] () ((6: 6 6))
listing:
(1: 1 1)
(2: 2 2)
(3: 3 3)
(4: 4 4)
(5: 5 5)
(6: 6 6)
```

## Doing it by hand

```bash
clink --db served.links --serve 127.0.0.1:7777     # terminal 1
clink --connect 127.0.0.1:7777 '() ((1 1))'        # terminal 2, text protocol
clink --connect 127.0.0.1:7777 --protocol binary   # list every link over binary
```

Text messages end with a line holding a single `.`, so you can also talk to
the server with `nc 127.0.0.1 7777`:

```
() ((2 2))
.
```

The wire formats are described in
[`docs/case-studies/issue-105`](../../docs/case-studies/issue-105/README.md).
