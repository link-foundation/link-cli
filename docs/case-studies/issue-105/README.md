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
| R3 | "binary version of links notation, that will have number of links in a message/packet" | `BinaryLinoProtocol` and `LinksPacket`. The header counts links (compact layout) or sections, each with its own count, as LEB128 (§4.2) |
| R4 | "0-256 links have 8 bit size. 257-65536 links have 16 bits size and so on" (8/16/32/64) | The width is the tier of the largest reference: 1, 2, 4 or 8 bytes. `--packed-widths` gives each section the narrowest width it needs instead (§4.2) |
| R5 | The binary protocol is "implemented as another decorator, for both C# and Rust" and can be "used in the same manner as unicode version … easily switchable" | Both protocols implement `LinoProtocol` (Rust trait) / `ILinoProtocol` (C# interface). `LinoConnection` decorates any byte stream with either one, so switching means swapping one value. The server detects the protocol of each message and answers in kind (§4.3) |
| R6 | "representation of numbers as defined in links themselves (see how we define numbers in our linksplatform projects)" | Default mapping: a number is `(Number unary(n))`, built from powers of two as in `Platform.Data.Doublets.Numbers.Unary` (§4.2) |
| R7 | "Or … use external references to directly support encoding of unicode sequences": "0-128 links have 8 bit size. 128-32768 … 16 bits" | `--external-references` / `BinaryLinoOptions.ExternalReferences`. Numbers and code points become `Platform.Data.Hybrid<T>` external references, and the internal range per width is halved (§4.2) |
| R8 | "at the end we can have improved format for storing variable length sequences … size, reference_1, reference_2 … Each variable link sequences is addressable itself by last link + 1" | `--arity` / `BinaryLinoOptions.Arity`, an `ArityRange` such as `2..3` or `1..`. A list of any allowed length is one link. A variable-arity section prefixes each link with its length, and every link keeps the address after the previous one (§4.2) |
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
- **"number of links in a message".** The compact layout stores the number
  of links N. The explicit layout stores the number of sections and a count
  for each. Either way, every count and width comes before the first
  reference, so a reader knows each width before it reads a reference. The
  widths follow from the largest reference in a section, not from N.
  Addresses `1..=5` are reserved marker points (§4.2).
- **"variable length sequences".** The issue puts sequences after the
  doublets, as a separate kind of record. We generalised that: any section
  may hold links of a fixed arity or of a variable one, so doublets,
  triplets and longer links mix freely. The default arity stays `2`, the
  doublets of linksplatform. The format accepts any arity from 1 up, and
  `1..` sets no maximum.
- **"decorator".** In C#, `NamedTypesDecorator`, `PinnedTypesDecorator` and the
  others wrap an `ILinks<T>`. A protocol here wraps a byte stream
  (`LinoConnection`), and each binary option is one setting on an options
  value that changes the encoder. No option needs another. This keeps the
  options composable without a separate wrapper type for every combination of
  three options. A reader still accepts every combination, because the header
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
| [link-foundation/links-queue](https://github.com/link-foundation/links-queue) | A "Binary Links Notation" codec (`docs/BINARY-NOTATION-SPEC.md`, JS and Rust): an 11-byte frame (`LNKQ` magic, version, flags, big-endian length), then a type byte per link, LEB128 ids and typed inline literals (strings, integers, floats, nested links). Its TCP server frames JSON queue operations with a 4-byte length | Nothing directly. Its links are self-describing trees of values; a store needs implicit addresses and fixed-width references, so the layouts differ (notes §8) |
| [link-foundation/links-notation](https://github.com/link-foundation/links-notation) | The LiNo parser and formatter (Rust crate and NuGet package) | Used directly. Upgraded 0.16.1 → 0.22.0 (§6.1) |

No project in these organisations defined a raw TCP protocol for LiNo, and
the one binary notation found (links-queue's) encodes value trees rather than
address-ordered links, so the framing and the binary layout are new. Where possible they reuse the
encodings above.

### Public protocols

| Protocol | Relevant idea | Influence |
|---|---|---|
| CBOR, [RFC 8949](https://www.rfc-editor.org/rfc/rfc8949.html) | Integer arguments inline, or in 1/2/4/8 following bytes | The 1/2/4/8-byte tiers, chosen once per link rather than once per value |
| MessagePack, PackStream (Bolt) | Width escalation through marker bytes (`0xcc..0xcf`, `INT_8..INT_64`) | Same as above |
| LEB128 / protobuf varints | 7 bits per byte, little-endian groups | Counts, section headers and link lengths |
| [PostgreSQL](https://www.postgresql.org/docs/current/protocol-overview.html), [MySQL](https://dev.mysql.com/doc/dev/mysql-server/latest/page_protocol_basic_packets.html) | Self-delimiting messages on a TCP stream | Every binary packet is self-delimiting: counts first, then fixed-size or size-prefixed records |
| SMTP (RFC 5321 §4.5.2) | Messages end with a line holding a single `.`, and leading dots are doubled | Text framing. It can be typed by hand in `nc` or `telnet` |
| Redis RESP | A text protocol that humans can type, where the first byte tells the type | Protocol detection by the first byte (§4.3) |
| LZW | Code width grows as the dictionary grows | `--packed-widths`: the width follows the references of each section |

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

The binary protocol writes each document as one packet of
[binary links notation](../../protocol/binary-links-notation.md). That document is the normative specification. It covers the
header, the sections, the widths, external references, packing and the LiNo
mapping. This section explains how the design meets the issue.

- **Count first (R3).** A packet starts with one header byte. Next comes a
  LEB128 count of links in the compact layout, or a count of sections and
  their headers in the explicit layout. After that come the links. So a
  packet is self-delimiting, and a reader knows every width before it reads a
  reference.
- **Sections.** A section is a run of consecutive addresses. It has a gap
  before it, an arity range (how many references each of its links holds)
  and a width. The common case is a document made of doublets, which is one
  section at address 6. It is written in the compact layout, so the header
  costs two bytes.
- **Width tiers (R4).** A width is the narrowest of 1, 2, 4 and 8 bytes that
  holds every reference of a section. By default every section uses the
  widest width in the packet, so all references have one size, as in the
  issue. `--packed-widths` gives each section its own width and splits
  sections where that saves bytes. A linear dynamic program chooses the
  split, and the result is never larger than the uniform layout.
- **Numbers as links (R6, the default).** Addresses `1` to `5` are the marker
  points `One`, `Number`, `String`, `List` and `Identified`, and they are
  never sent. A number is `(Number unary(n))`, built from powers of two as in
  `Platform.Data.Doublets.Numbers.Unary`. A string is `(String code-point…)`.
  The document is the list of its top-level links, stored last.
- **External references (R7, `--external-references`).** Numbers and code
  points become `Platform.Data.Hybrid<T>` values: `v ≥ 1` is `2^w − v` and `0`
  is `2^(w−1)`. The top bit marks a value as external, so internal addresses
  lose one bit: 8-bit references reach `0..=127`.
- **Variable-length links (R8, `--arity`).** The arity range is the set of
  link lengths the encoder may use. The default, `2`, writes only doublets,
  so a list becomes the cons chain `(marker (e1 (e2 (… (en 0)))))`. With
  `--arity 2..3`, a list of three elements becomes one triplet. With
  `--arity 1..`, any list becomes one link. A variable-arity section prefixes
  each link with its length minus the section minimum. That is the issue's
  `size, reference_1, reference_2 …` record. Every link keeps its own
  address, one after the previous link, as the issue asks.
- **Untrusted input.** `DecodeLimits` caps links (2²²), references (2²⁴),
  expanded LiNo nodes, nesting depth (1024) and the text message size
  (64 MiB). The decoder rejects:
  - unknown header versions;
  - LEB128 overflow;
  - lengths outside a section's arity;
  - forward references;
  - misplaced markers;
  - invalid code points;
  - truncated packets;
  - trailing bytes.

### 4.3 Decorators and switching (R5, R9)

```rust
let mut connection = LinoConnection::new(stream, BinaryLinoProtocol::with_options(
    BinaryLinoOptions::default()
        .with_external_references(true)
        .with_arity(ArityRange::at_least(1))));
// …or TextLinoProtocol::new(); the rest of the code is unchanged.
```

```csharp
using var client = LinksClient.Connect("127.0.0.1", 7777,
    new BinaryLinoProtocol(new BinaryLinoOptions().WithExternalReferences().WithArity(ArityRange.AtLeast(1))));
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
clink --connect 127.0.0.1:7777 --external-references --arity 1.. --packed-widths '((1: 1 1)) ((1: 1 2))'
```

On the client, any binary option implies `--protocol binary`, and
`--protocol text` together with a binary option is an error. Using `--serve`
and `--connect` together is also an error.

### 4.7 Store archive

The review of PR #106 asked for a full import and export of a store in
binary links notation. A store archive is two packets: the links, each at its
own address with holes for unused ones, and then the names as external code
points. Both use packed widths, so a store of small addresses costs about two
bytes per link
([specification](../../protocol/binary-links-notation.md#10-store-archive)).

```bash
clink --db family.links --export-binary family.bin     # also --binary-output, --binary-out
clink --db copy.links --import-binary family.bin       # also --binary-input, --binary-in
```

The archive works on the links interface only, so it exports and imports a
remote store (`RemoteLinks`) as well as a local one. Import validates the
whole archive before it writes, then recreates every address, link and name.
Exporting the imported store gives the same bytes again.

## 5. Verification

### Golden vectors

[`docs/protocol/binary-links-notation-vectors.txt`](../../protocol/binary-links-notation-vectors.txt)
holds 84 documents and 12 raw packets. Every document is encoded under all
12 option sets: plain or external references, arity `2`, `2..3` or `1..`, and
uniform or packed widths. The Rust and C# tests both assert every byte, in
both directions. A few of the vectors:

| Document | ext | arity | Bytes |
|---|---|---|---|
| (empty) | – | any | `10 00` |
| `() ((1 1))` | – | `2` | `10 07 02 01 06 06 07 00 04 08 00 09 0a 00 04 0b` |
| `() ((1 1))` | – | `1..` | `12 04 24 05 02 10 01 20 01 10 01 02 01 06 06 07 00 08 09` |
| `() ((1 1))` | ✓ | `2` | `11 06 ff ff 06 00 04 07 00 08 09 00 04 0a` |
| `() ((1 1))` | ✓ | `1..` | `13 01 1c 05 01 04 01 ff ff 00 06 01 00 07 00 08` |
| `hi` | ✓ | `2` | `11 05 97 00 98 06 03 07 08 00 04 09` |
| `hi` | ✓ | `1..` | `13 02 34 05 01 10 01 03 98 97 06` |

[The specification](../../protocol/binary-links-notation.md#72-example)
decodes the first two of these byte by byte.

### Sizes

The [sizes table](../../protocol/binary-links-notation.md#9-sizes) of the
specification compares text with every option set. In short:

| Document | text | plain | ext | ext, `1..` | ext, `1..`, packed |
|---|---|---|---|---|---|
| `() ((1 1))` | 13 | 16 | 14 | 16 | 16 |
| `((1: 1 1)) ((1: 1 2))` | 24 | 38 | 32 | 25 | 25 |
| `() ((child: father mother))` | 30 | 132 | 52 | 41 | 41 |
| `() ((1000 70000))` | 20 | 70 | 50 | 34 | 23 |

The pure-links mapping (R6) is not meant to be smaller than text. It
transmits a document as nothing but links, which a links store can import
without any parser. The options that make a packet compact are external
references, a wider arity and packed widths. Text stays smaller for documents
made of names, because each name costs a string link with a marker and one
reference per code point. The binary protocol's advantage is that it needs no
parsing and has a fixed structure, not that it is smaller.

### Automated tests

- Rust:
  - `rust/tests/protocol_packet_tests.rs`: both codecs. For the binary links notation it covers the golden vectors, the corpus round trip under all 12 option sets, width tiers and Hybrid values. It also covers packing (packed widths are never larger than uniform ones), arity ranges, hand-built packets, malformed input and limits. For text it covers framing, protocol detection, and deep nesting in linear time.
  - `rust/tests/protocol_tcp_tests.rs`: CRUD over every protocol, mixed clients, errors, CRLF, malformed input, protocol restriction, concurrency and shutdown.
  - `rust/tests/cli_tcp_tests.rs`: `clink --serve` / `--connect` end to end.
  - `rust/tests/remote_links_tests.rs`: the same proof for the Rust `RemoteLinks`, including the shared conversation.
  - `rust/tests/store_archive_tests.rs`: the golden archive, a round trip of links, holes and names, a remote store, malformed archives, and the CLI options.
- C#:
  - `BinaryLinksNotationTests`: a port of `protocol_packet_tests.rs`, checked against the same golden vectors.
  - `LinoProtocolCodecTests`: the same corpus under all 12 option sets, and the text framing.
  - `LinksServerTests`: CRUD over every protocol, identical text and binary replies, errors, CRLF, malformed input, protocol restriction, concurrent clients, shutdown, and shutdown racing a new connection.
  - `RemoteLinksTests`: every `INamedTypesLinks` call, the query processor and the raw `ILinks` interface give the same answers on a local store and over every protocol. It also covers the shared conversation, malformed operations and replies, and lost connections.
  - `CliTcpIntegrationTests`: `clink --serve` / `--connect` end to end, including option validation.
  - `StoreArchiveTests` and `CliStoreArchiveTests`: ports of `store_archive_tests.rs`, checked against the same golden archive.
- Cross-language interop: [`examples/tcp/run-interop.sh`](../../../examples/tcp/run-interop.sh) runs a Rust server with a C# client, and a C# server with a Rust client. It covers text and every binary option, and both directions print identical results.
- Store archive interop: [`examples/archive/run-interop.sh`](../../../examples/archive/run-interop.sh) exports a store with one port and imports it with the other, in both directions, and checks that the stores are equal.

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
