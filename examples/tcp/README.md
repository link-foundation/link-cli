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
[--arity 2..3] () ((4: 4 4))
[--arity 1..] () ((5: 5 5))
[--packed-widths] () ((6: 6 6))
[--external-references --arity 1.. --packed-widths] () ((7: 7 7))
listing:
(1: 1 1)
(2: 2 2)
(3: 3 3)
(4: 4 4)
(5: 5 5)
(6: 6 6)
(7: 7 7)
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

Besides queries, a server answers one request per call of the links
interface, which is what `RemoteLinks` sends. After `() ((1 1) (2 2) (1 2))`,
two links use `1`:

```bash
clink --connect 127.0.0.1:7777 '(count: (* 1))'
(count: 2)
```

[`docs/protocol/links-operations.txt`](../../docs/protocol/links-operations.txt)
lists every such request with its reply.

The wire formats are described in
[`docs/case-studies/issue-105`](../../docs/case-studies/issue-105/README.md).
