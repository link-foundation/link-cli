# Case study: LiNo substitution operations over TCP (issue #105)

Issue: [#105 "LINO API or LINKQL over TCP"](https://github.com/link-foundation/link-cli/issues/105)
Pull request: [#106](https://github.com/link-foundation/link-cli/pull/106)

This folder contains:

- `README.md`: this analysis. It covers the requirements, prior art, design, wire format, verification and the remaining limits.
- [`research-notes.md`](research-notes.md): raw notes on the linksplatform, link-foundation and link-assistant repositories and on public wire protocols, with quotes and links.
- `github-data/`: the issue, its comments and the pull request as JSON, captured on 2026-10-04.

## 1. The problem

link-cli already runs LiNo *substitution* queries, `(before) (after)`, against
a doublets store. Those queries cover create, read, update and delete. Until
now they only ran in-process, from the command line or through the library.
Other DBMSs expose their query language over a TCP socket: PostgreSQL on 5432,
MySQL on 3306, Redis on 6379 and Neo4j Bolt on 7687. Issue #105 asks for the
same thing for LiNo, with two protocols:

- A UTF-8 text protocol.
- A compact binary protocol whose reference width grows with the number of links in the message.

The protocols must be interchangeable and every optional feature must be a
decorator, in both the C# and the Rust port.

## 2. Requirements

| # | Requirement (from the issue text) | Where it is addressed |
|---|---|---|
| R1 | "lightweight protocols to transfer links notation representing substitution operations to allow all CRUD operations over TCP/IP" | `LinksServer` / `LinksClient` in both ports. A request is a substitution query and the empty request reads everything (§4.4). `clink --serve` / `--connect` (§4.6). `RemoteLinks` serves the links interface itself (§4.5) |
| R2 | "UTF-8 based text only links notation" | `TextLinoProtocol` (§4.1) |
| R3 | "binary version of links notation, that will have number of links in a message/packet" | `BinaryLinoProtocol` and `LinksPacket`. The header carries N (doublets) and M (sequences) as LEB128 (§4.2) |
| R4 | "0-256 links have 8 bit size. 257-65536 links have 16 bits size and so on" (8/16/32/64) | The width is the tier of the largest address: 1, 2, 4 or 8 bytes. `--progressive-widths` grows the width per address instead (§4.2.3) |
| R5 | The binary protocol is "implemented as another decorator, for both C# and Rust" and can be "used in the same manner as unicode version … easily switchable" | Both protocols implement `LinoProtocol` (Rust trait) / `ILinoProtocol` (C# interface). `LinoConnection` decorates any byte stream with either one, so switching means swapping one value. The server detects the protocol of each message and answers in kind (§4.3) |
| R6 | "representation of numbers as defined in links themselves (see how we define numbers in our linksplatform projects)" | Default mapping: a number is `(Number unary(n))`, built from powers of two as in `Platform.Data.Doublets.Numbers.Unary` (§4.2.4) |
| R7 | "Or … use external references to directly support encoding of unicode sequences": "0-128 links have 8 bit size. 128-32768 … 16 bits" | `--external-references` / `BinaryLinoOptions.ExternalReferences`. Numbers and code points become `Platform.Data.Hybrid<T>` external references, and the internal range per width is halved (§4.2.5) |
| R8 | "at the end we can have improved format for storing variable length sequences … size, reference_1, reference_2 … Each variable link sequences is addressable itself by last link + 1" | `--sequences` / `BinaryLinoOptions.Sequences`. A section of `size ref…` records follows the fixed doublets, and sequence *k* has address `6 + N + k` (§4.2.6) |
| R9 | "all these protocols options are optional, and come as decorators" | Every option is off by default and can be switched on independently, with `With…` builders. `AcceptedProtocols` lets a server restrict the protocol (§4.3) |
| R10 | "Reuse all the best experience … from other repositories of link-assistant, link-foundation, and linksplatform" | §3 and `research-notes.md`. Hybrid encoding, unary numbers, the linksql CRUD table and the RFC 9457-like error shape were reused |
| R11 | "collect data … to `./docs/case-studies/issue-{id}` … search online … list of each and all requirements … propose possible solutions and solution plans … check known existing components/libraries" | This folder |
| R12 | "plan and execute everything in this single pull request" | PR #106 |

### Reading of ambiguous points

- **"0-256" / "0-128" boundaries.** An 8-bit field holds 256 values. Address
  `0` is null, so plain 8-bit references reach addresses `0..=255`. With the
  Hybrid top bit, 8-bit internal references reach `0..=127`. The issue's
  boundaries are each off by one. We follow the arithmetic, which is also what
  `Platform.Data.Hybrid<T>` does.
- **"number of links in a message".** The header stores the number of fixed
  doublets N and, when the sequence section is on, the number of sequences M.
  The width follows from the largest address, `5 + N + M`, not from N alone.
  Addresses `1..=5` are reserved marker points (§4.2.2), so a packet always
  knows its own maximum width before it reads any reference.
- **"decorator".** In C#, `NamedTypesDecorator`, `PinnedTypesDecorator` and the
  others wrap an `ILinks<T>`. A protocol here wraps a byte stream
  (`LinoConnection`), and each binary option is one flag on an options value
  that changes the encoder. Neither option needs the other. This keeps the
  options composable without a separate wrapper type for every combination of
  three flags. A reader still accepts every combination, because the header
  announces the options in use.

## 3. Prior art

Full notes with quotes and file paths are in [`research-notes.md`](research-notes.md).

### Inside the organisations

| Project | What it has | What we took |
|---|---|---|
| [linksplatform/Data](https://github.com/linksplatform/Data) `Hybrid<T>` | External references: top bit set means external, value `v` stored as `-v` (two's complement) and `0` stored as `2^(n-1)` (`ExternalZero`) | The exact external encoding at every width (8/16/32/64), so the 32-bit wire value equals `Hybrid<uint>` |
| [linksplatform/Numbers](https://github.com/linksplatform/Numbers) and `Data.Doublets` unary converters | `2^0 = 1`, `2^k = (2^(k-1) 2^(k-1))`, and a number is a nested sum of powers of two | The in-band number representation (R6) |
| `Data.Doublets.Sequences` `BalancedVariantConverter`, `StringToUnicodeSequenceConverter` | Strings as sequences of code points under a type marker | Strings as `(String code points…)` |
| [link-foundation/linksql](https://github.com/link-foundation/linksql) | HTTP + SSE only, with "Links Notation as the wire protocol" and the CRUD table `() ((s t))` / `((a b)) ((a c))` / `((a b)) ()` | The request semantics. No TCP transport existed to reuse |
| [link-foundation/lino-rest-api](https://github.com/link-foundation/lino-rest-api) | HTTP REST with `text/lino-line` and RFC 9457 problem details written in LiNo | The `(error: 'message')` reply, a minimal form of the same idea |
| [linksplatform/IdDistributorServer](https://github.com/linksplatform/IdDistributorServer), IdClient | A C TCP ping-pong benchmark with fixed 32-byte messages and `TCP_NODELAY` | Only the low-latency stance: `NoDelay` is set on every connection |
| [linksplatform/Protocols](https://github.com/linksplatform/Protocols) | UDP string sender and receiver (`Platform.Protocols.Udp`) | Nothing reusable for TCP framing |
| [link-foundation/links-notation](https://github.com/link-foundation/links-notation) | The LiNo parser and formatter (Rust crate and NuGet package) | Used directly. Upgraded 0.16.1 → 0.22.0 (§6.1) |

No project in these organisations defined a raw TCP protocol for LiNo, so the
framing and the binary layout are new. Where possible they reuse the
encodings above.

### Public protocols

| Protocol | Relevant idea | Influence |
|---|---|---|
| CBOR, [RFC 8949](https://www.rfc-editor.org/rfc/rfc8949.html) | Integer arguments inline, or in 1/2/4/8 following bytes | The 1/2/4/8-byte tiers, chosen once per link rather than once per value |
| MessagePack, PackStream (Bolt) | Width escalation through marker bytes (`0xcc..0xcf`, `INT_8..INT_64`) | Same as above |
| LEB128 / protobuf varints | 7 bits per byte, little-endian groups | Header counts N and M |
| [PostgreSQL](https://www.postgresql.org/docs/current/protocol-overview.html), [MySQL](https://dev.mysql.com/doc/dev/mysql-server/latest/page_protocol_basic_packets.html) | Self-delimiting messages on a TCP stream | Every binary packet is self-delimiting: counts first, then fixed-size or size-prefixed records |
| SMTP (RFC 5321 §4.5.2) | Messages end with a line holding a single `.`, and leading dots are doubled | Text framing. It can be typed by hand in `nc` or `telnet` |
| Redis RESP | A text protocol that humans can type, where the first byte tells the type | Protocol detection by the first byte (§4.3) |
| LZW | Code width grows as the dictionary grows | `--progressive-widths` |

### Libraries considered

- **links-notation** (Rust `links-notation`, NuGet `Link.Foundation.Links.Notation`) is used for all text parsing and formatting.
- **lino-objects-codec** encodes JSON-like objects as LiNo. It is not needed, because requests and replies are already LiNo documents.
- **tokio / async-std** were not used. The Rust store is single-threaded, and a thread per connection with a store lock is simpler and enough for a CLI. The same model is used in C# with `TcpListener` and threads.
- **protobuf, Cap'n Proto, FlatBuffers** were rejected. They need a schema and would encode a LiNo tree as generic messages. The issue asks for a links-native layout with address-sized references.

## 4. Design

Both ports implement the same specification. The golden vectors (§5) prove
that they produce identical bytes.

### 4.1 Text protocol

```text
message  = *line "." CRLF-or-LF
line     = (escaped LiNo text) CRLF-or-LF  ; a line starting with "." gets one more "."
```

- The payload is canonical LiNo with one top-level link per line, encoded as UTF-8.
- Readers accept `\n` and `\r\n`.
- A request and its reply use the same framing. An empty message (just `.`) is the empty document.
- Caveat: a quoted reference that contains `\r\n` arrives as `\n`. The binary protocol carries such references exactly.

A session in `nc`:

```text
() ((1 1))
.
() ((1: 1 1))
.
```

### 4.2 Binary protocol

#### 4.2.1 Packet layout

```text
byte 0      0x10 | flags        high nibble 1 = format version 1
                                bit 0     external references (Hybrid encoding)
                                bit 1     sequence section present
                                bits 2-3  log2 of the minimum width in bytes
LEB128      N                   number of fixed doublets
LEB128      M                   number of sequences (only when bit 1 is set)
N times     source target       one fixed doublet, refs width(a) bytes each
M times     size ref_1 … ref_n  one sequence, size and refs width(a) bytes each
```

All references are little-endian. A packet is self-delimiting, so it needs no
length prefix.

#### 4.2.2 Addresses

| Address | Meaning |
|---|---|
| `0` | null, the empty link `()` |
| `1` | `One`, the unary 1 |
| `2` | `Number` marker |
| `3` | `String` marker |
| `4` | `List` marker |
| `5` | `Identified` marker (a link with an id) |
| `6 … 5+N` | fixed doublets |
| `6+N … 5+N+M` | sequences ("last link + 1", R8) |

Markers are never transmitted. A link may only refer to links before it, so a
decoder resolves everything in one pass. The last link is the document root.

#### 4.2.3 Width tiers (R4)

`tier(a)` is the smallest of 1, 2, 4 and 8 bytes that can hold address `a`.
Every reference of the link at address `a` uses
`width(a) = max(min_width, tier(a))` bytes.

- **Default (uniform).** `min_width` is the tier of the highest address, so
  one width is used for the whole packet:

  | Highest address | Plain | With external references |
  |---|---|---|
  | up to 255 / 127 | 8-bit | 8-bit |
  | up to 65 535 / 32 767 | 16-bit | 16-bit |
  | up to 2³²−1 / 2³¹−1 | 32-bit | 32-bit |
  | larger | 64-bit | 64-bit |

  The first column gives the limit for plain references, the second for references with external references on.

- **`--progressive-widths`.** `min_width` is 1, so the first links use 8-bit
  references even in a large packet. The width only grows when the address
  grows. Since a link at address `a` can only refer to addresses below `a`,
  `tier(a)` always fits.

#### 4.2.4 Numbers as links (R6, the default)

| LiNo | Links |
|---|---|
| `()` | `0` |
| number `n` | `(Number unary(n))`. `unary(0) = 0`, `2^0 = One`, `2^k = (2^(k-1) 2^(k-1))`, and other values are right-nested sums of powers of two from the highest bit down |
| other reference | `(String code-point…)`, with each code point a unary number |
| link of two values, no id | a plain doublet |
| link of any other arity, no id | `(List e…)` |
| link with an id | `(Identified id value…)` |
| document | `(List top-level-link…)`, the root |

Identical sub-links are emitted once, because links are content-addressed.

#### 4.2.5 External references (R7, `--external-references`)

Numbers and code points travel as Hybrid external references instead of
unary links. At width `w` bits:

- value `v ≥ 1` is sent as `2^w − v`
- `0` is sent as `2^(w−1)`

This is exactly `Platform.Data.Hybrid<T>`. The top bit marks a reference as
external, so internal addresses lose one bit: 8-bit references reach `0..127`.

#### 4.2.6 Sequence section (R8, `--sequences`)

Without this option, lists, strings and identified links are cons chains of
doublets: `(marker (e1 (e2 (… (en 0)))))`. With it, they become one
variable-length record `size marker e1 … en`, and a plain list has no marker.

Fixed doublets may only refer to earlier fixed doublets. So a two-value link
that holds a sequence becomes a two-element sequence.

#### 4.2.7 Decoding untrusted input

`DecodeLimits` caps the following:

- the number of links: 2²²
- the total sequence items: 2²⁴
- the expanded LiNo nodes
- the nesting depth
- the text message size: 64 MiB

The decoder also rejects:

- a forward reference or a reference to itself
- an unsupported header byte, meaning an unknown version
- a width too small for an address
- a truncated packet

### 4.3 Decorators and switching (R5, R9)

```rust
let mut connection = LinoConnection::new(stream, BinaryLinoProtocol::with_options(
    BinaryLinoOptions::default().with_external_references(true).with_sequences(true)));
// …or TextLinoProtocol::new(); the rest of the code is unchanged.
```

```csharp
using var client = LinksClient.Connect("127.0.0.1", 7777,
    new BinaryLinoProtocol(new BinaryLinoOptions().WithExternalReferences().WithSequences()));
var reply = client.QueryText("() ((1 1))");
```

Protocol detection: a text message never starts with a byte in `0x10..0x1F`,
and a binary packet always does. `LinksServer` therefore reads each message,
detects its protocol and replies in the same protocol, with the same options.
`AcceptedProtocols.Text` / `Binary` (`--protocol` on `--serve`) restricts the
server to one protocol.

### 4.4 Request semantics

| Request | Reply |
|---|---|
| empty document | every link, `(index: source target)` per line, named where a name exists |
| substitution query | one `(before) (after)` line per change, as with `clink --changes` |
| failing query | `(error: 'message')`. The connection stays usable |
| malformed message | `(error: 'message')` in text, then the server closes the connection, because the stream may be out of sync |

Requests run one at a time under a store lock. Each connection has its own
thread for parsing and formatting.

### 4.5 Remote stores

A substitution query is one way to use a store; the links interface is the
other. `RemoteLinks` implements that interface — `INamedTypesLinks<uint>` in
C#, `Links`, `Doublets` and `NamedTypeLinks` in Rust — by sending one
`LinksOperation` document per call. So a local store and a served one are
swappable, the query processor included:

| Request | Reply |
|---|---|
| `(count: (index source target))` | `(count: N)` |
| `(each: (index source target))` | one `(index: source target)` per match |
| `(create: (source target))` | `() ((index: source target))` |
| `(update: (index source target))` | `((index: s t)) ((index: source target))` per changed link |
| `(delete: index)` | `((index: source target)) ()` per removed link |
| `(get-name: link)` | `(name: 'text')`, or nothing |
| `(set-name: (link 'text'))` | `(link: N)`, the address of the name |
| `(get-by-name: 'text')` | `(link: N)`, or nothing |
| `(remove-name: link)` | nothing |

A restriction holds up to three parts, and `*` matches anything. An operation
holds exactly one value, and a substitution query needs a restriction *and* a
substitution, so the two never collide on one server.

A write replies with the **net change** of every link it touched, the way
`clink --changes` reports a query, not with the steps the store took. The
steps differ between the ports: a cascading delete in C# clears a usage to
`(0 0)` before it removes it, while doublets-rs 0.5.0 removes it directly.
The net changes are the same in both, so the replies are too.

[`docs/protocol/links-operations.txt`](../../protocol/links-operations.txt)
records one conversation with a fresh server, covering every operation, a
cascade and the errors. Both test suites replay it over every protocol, so a
client of either port is proven to understand a server of the other.

### 4.6 Command line

```bash
clink --db data.links --serve 127.0.0.1:7777 [--protocol text|binary] [--auto-create-missing-references] [--trace]
clink --connect 127.0.0.1:7777 '() ((1 1))'                      # text
clink --connect 127.0.0.1:7777 --protocol binary                 # list all links over binary
clink --connect 127.0.0.1:7777 --external-references --sequences --progressive-widths '((1: 1 1)) ((1: 1 2))'
```

On the client, any binary option implies `--protocol binary`, and
`--protocol text` together with a binary option is an error. Using `--serve`
and `--connect` together is also an error.

## 5. Verification

### Golden vectors

The Rust and C# tests both assert these bytes. The vectors use the uniform
width.

| Document | ext | seq | Bytes |
|---|---|---|---|
| `() ((1 1))` | – | – | `10 07 02 01 06 06 07 00 04 08 00 09 0a 00 04 0b` |
| `() ((1 1))` | – | ✓ | `12 02 03 02 01 06 06 01 07 02 00 08 01 09` |
| `() ((1 1))` | ✓ | – | `11 06 ff ff 06 00 04 07 00 08 09 00 04 0a` |
| `() ((1 1))` | ✓ | ✓ | `13 01 03 ff ff 01 06 02 00 07 01 08` |
| `hi` | ✓ | – | `11 05 97 00 98 06 03 07 08 00 04 09` |
| `hi` | ✓ | ✓ | `13 00 02 03 03 98 97 01 06` |
| (empty) | – | – | `10 00` |
| (empty) | ✓ | ✓ | `13 00 00` |

How the first vector decodes:

- header `0x10`, N = 7
- `6 = (Number One)` = 1
- `7 = (6 6)` = `(1 1)`
- `8 = (7 0)`
- `9 = (List 8)` = `((1 1))`
- `10 = (0 9)` = `() ((1 1))`
- `11 = (10 0)`
- `12 = (List 11)`, the document

### Sizes

Output of `cargo run --example lino_binary_dump -- '<doc>'`, in bytes. The
text size includes the `\n.\n` terminator.

| Document | text | plain | seq | ext | ext+seq |
|---|---|---|---|---|---|
| `() ((1 1))` | 13 | 16 | 14 | 14 | 12 |
| `((1: 1 1)) ((1: 1 2))` | 24 | 38 | 28 | 32 | 22 |
| `() ((child: father mother))` | 30 | 132 | 118 | 52 | 38 |
| `() ((1000 70000))` | 20 | 70 | 68 | 50 | 39 |

The pure-links mapping (R6) is not meant to be smaller than text. It
transmits a document as nothing but links, which a links store can import
without any parser. External references and sequences are the options that
make the packet compact, and with both on, the binary form is the smallest
for numeric links. Text stays smaller for documents made of names or large
numbers, because:

- each name costs a sequence with a marker and one reference per code point;
- each number needs a whole external reference at the packet's width.

The binary protocol's advantage is that it needs no parsing and has a fixed
structure, not that it is smaller.

### Automated tests

- Rust:
  - `rust/tests/protocol_packet_tests.rs`: codecs, the golden vectors, the corpus round trip under all 8 option sets, limits, malformed input, and deep nesting in linear time.
  - `rust/tests/protocol_tcp_tests.rs`: CRUD over every protocol, mixed clients, errors, CRLF, malformed input, protocol restriction, concurrency and shutdown.
  - `rust/tests/cli_tcp_tests.rs`: `clink --serve` / `--connect` end to end.
  - `rust/tests/remote_links_tests.rs`: the same proof for the Rust `RemoteLinks`, including the shared conversation.
- C#:
  - `LinoProtocolCodecTests` (34 tests): the same corpus and golden vectors.
  - `LinksServerTests` (10 tests): CRUD over every protocol, identical text and binary replies, errors, CRLF, malformed input, protocol restriction, concurrent clients, shutdown, and shutdown racing a new connection.
  - `RemoteLinksTests`: every `INamedTypesLinks` call, the query processor and the raw `ILinks` interface give the same answers on a local store and over every protocol. It also covers the shared conversation, malformed operations and replies, and lost connections.
  - `CliTcpIntegrationTests` (8 tests): `clink --serve` / `--connect` end to end.
- Cross-language interop: [`examples/tcp/run-interop.sh`](../../../examples/tcp/run-interop.sh) runs a Rust server with a C# client, and a C# server with a Rust client. It covers text and every binary option, and both directions print identical results.

## 6. Findings along the way

### 6.1 links-notation 0.16.1 parsed nested input in exponential time

The time to parse `((((…a…))))` doubled with every level of nesting. A
request of a few dozen bytes could keep a server thread busy for minutes,
which would be a denial of service for anything listening on a socket. This
is upstream issue
[links-notation#314](https://github.com/link-foundation/links-notation/issues/314),
fixed in 0.21.3.

Both ports now use **0.22.0**. Its parse time is linear and it rejects deeper
nesting than its limit. The regression test (`deeply_nested_text_parses_in_linear_time`
in Rust and `DeeplyNestedTextParsesInLinearTime` in C#) parses 40 levels
correctly, and rejects 100 000 levels quickly instead of hanging.

### 6.2 One canonical model in both ports

links-notation represents `a` and `(a)` the same way: an unnamed group
holding one reference. `parse_document` / `LinoFormat.ParseDocument`
collapse such a group to the reference itself, recursively.
`format_document` writes `((a))` when a one-reference link must survive.
With this rule, `parse(format(doc)) == doc` holds in both ports, and a
document decoded from the same bytes compares equal in Rust and C#. The tool
in `examples/lino-structure-probe` and `rust/examples/lino_structure_probe.rs`
prints the raw parser output, which was used to confirm this.

### 6.3 Behaviour differences between the ports' query processors

A server replies with whatever its query processor reports, so every
difference between the processors became a difference between a C# and a Rust
server. Comparing the two `--changes` reports query by query
(`experiments/compare-cli-changes.sh`, and the parity harness in
[`../issue-100/evidence/cli-parity/run.sh`](../issue-100/evidence/cli-parity/run.sh),
now 41 scenarios that all agree) found these, all fixed here:

- **Creations were not all reported.** The C# processor did not report a
  named composite link it created, so `() ((a: a a))` got an empty reply from
  a C# server and `() ((a: a a))` from a Rust one. Both now report every link
  a query creates, once, as the creation of its final value.
- **New addresses were predicted wrongly.** Reference validation assumed a new
  link gets the lowest free address, but both stores reuse the address freed
  last first. Both now ask the store: create the links, note their addresses,
  delete them again in reverse.
- **Errors were worded differently.** C# printed an unhandled exception with
  its stack trace; it now prints one `Error: Query error: ...` or
  `Error: Parse error: ...` line, as Rust does, and keeps the stack trace for
  `--trace`.
- **A merge blanked usages in C#.** An update that turns a link into a
  duplicate of another merges it into that other link, re-pointing whatever
  used the merged-away address. After `() ((1 1) (2 2))` and
  `((1: 1 1)) ((1: 1 2))`, the query `((2: 2 2)) ((2: 1 2))` merges 2 into 1:

  | | link 1 | link 2 |
  |---|---|---|
  | Rust (doublets-rs) | `(1: 1 1)`, its target re-pointed from 2 to 1 | `(2: 1 2)`, written by the query |
  | C# before | `(1: 1 0)`, its target blanked | `(2: 1 2)` |

  The cause is upstream: `MergeUsages` in Platform.Data.Doublets 0.18.1 builds
  `new Link<T>(a, b)`, which binds to the `params` constructor and means
  `(index: a, source: b, target: 0)`, not `(source: a, target: b)`. It was
  already reported as
  [Data.Doublets#515](https://github.com/linksplatform/Data.Doublets/issues/515)
  during issue #100 and kept as the harness's one known difference. It is
  still not released, so the C# library now composes its stores with
  `DecorateWithAutomaticUniquenessAndUsagesRepointing()`: the same three
  layers as `DecorateWithAutomaticUniquenessAndUsagesResolution()`, with
  `LinksUniquenessAndUsagesRepointingResolver` on top, which replaces the
  merged-away address in both halves of each usage in one update.
  `LinksUniquenessAndUsagesRepointingResolverTests` fails on four of five
  tests against the upstream resolver and passes against this one.

Deleting a link also deletes the links that refer to it, transitively, in
both ports. This is by design, not a difference: a link never refers to an
address that no longer exists. For example, `((2: 2 2)) ()` with `(3: 1 2)`
present replies with two deletions. The behaviour is documented in
[HOW-IT-WORKS](../../HOW-IT-WORKS.md) and proven by the
`DeleteCascades*` tests in C# and the `*cascade*` tests in Rust.

## 7. Risks and remaining limits

- **No authentication or encryption.** The server is meant for trusted
  networks or localhost, like a development database. For anything else, put
  it behind TLS (stunnel or an SSH tunnel) or bind it to `127.0.0.1`. Adding
  TLS would mean wrapping the stream, which the `LinoConnection` design
  already allows.
- **Requests are serialized.** Requests run one at a time under a store lock,
  so throughput is limited by the store, not by the network.
- **No streaming of huge results.** A reply is one message. The decode limits
  bound what a peer can send.
- **Version 1 only.** The high nibble of the header is the version, and
  `0x2_` … `0xF_` are reserved for future layouts, for example a different
  marker set.

## 8. Possible next steps

1. A TLS decorator (`TlsLinoConnection`) once a use case needs it.
2. Subscriptions over the same connection: server-pushed `(before) (after)` messages, similar to linksql's SSE `GET /subscribe`.
3. Exposing the protocol from linksql's engine, so linksql gains the TCP transport its spec lacks.
4. A `Platform.Protocols.Tcp` package extracted from `LinksServer` / `LinksClient` if other projects need it.
