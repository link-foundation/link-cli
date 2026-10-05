<!-- Collected on 2026-10-04 while solving issue #105. Paths under /tmp/research are local clones of the named repositories. Version numbers are as of that date: link-cli has since moved to links-notation 0.22.0, see README.md §6.1. -->

# Research notes for link-cli issue #105 ("LINO API or LINKQL over TCP")

Clones live in `/tmp/research/*`. Also cloned here: `Data`, `Data.Doublets`, `doublets-rs`,
`Converters`, `data-rs` (Rust `platform-data`), and `links-notation-0.16` (tag `rust_0.16.1` =
`csharp_0.16.1`, commit `f0193ce`). The main `links-notation` clone is at 0.22.0.

Key takeaways:

- No linksplatform or link-foundation project defines a raw TCP protocol for LiNo yet. linksql only
  has HTTP + SSE (JS only). IdDistributorServer/IdClient is a TCP ping-pong benchmark with fixed
  32-byte messages, not a real protocol. link-cli itself has no networking code (`grep TcpListener|TcpStream|tokio|Socket` → nothing).
- Hybrid / external references: **top bit set ⇔ external**. The value is stored as **two's-complement
  negation** (`-v mod 2^N`), not as flag | magnitude. Raw value 0 is a special case, encoded as
  `ExternalZero = 2^(N-1)`.
- CBOR's argument widths (inline <24, then 1/2/4/8 following bytes) and MySQL's length-encoded
  integers are the closest prior art to the issue's width-escalation scheme.

---

## 1. linksql (`/tmp/research/linksql`, v0.1.0, spec "Version 0.1 (draft)")

Ports: `js/` (reference: engine, HTTP server, client, CLI), `rust/`, `python/`, `csharp/` (engine,
wire codec, schema only). Spec §13: *"`rust/`, `python/`, and `csharp/` ... do not ship the HTTP
server, client or CLI."*

### Transport
- **There is no TCP/binary transport.** The only transport is §11 "HTTP protocol" (OPTIONAL):

| Method + path | Request | Response |
|---|---|---|
| `GET /` | — | `{ name, version, links }` |
| `POST /query` | JSON `{ "query": "…" }` or raw LiNo body | query report (§6) |
| `GET /links` | — | `{ links: [...] }` |
| `GET /introspect` | — | introspection snapshot |
| `POST /import` | raw LiNo body | `{ imported }` |
| `GET /export` | — | `text/plain` canonical LiNo |
| `GET /subscribe?pattern=` | restriction | `text/event-stream` (SSE) |

  Schema servers add `GET /schema`, `POST /query/<name>`, `GET /subscribe/<name>`.
- Errors: *"Unknown routes return `404` with `{ error }`. Any thrown error returns `400` with `{ error: message }`."*
  (`js/src/server.js:199-201`: `sendData(req, res, 400, { error: error.message })`).
- SSE (§11.1): opens with the comment `: linksql stream open`, then one `data: <json>` frame per change,
  where the payload is `{ operation, matching }`.
- Server: `js/src/server.js` uses Node's `http.createServer` with no framework. Client: `js/src/client.js` uses `fetch`.

### Wire format (§2.5 "Links Notation as the wire protocol")
> "Every structured value that crosses a network boundary (query reports, link lists, introspection
> and schema documents, subscription events) is encoded as Links Notation text rather than JSON."
- The default content type is `application/lino`. JSON is opt-in via `Accept: application/json`. If a client accepts both, LiNo wins.
- Codec: `lino-objects-codec` (`js/src/protocol.js`: `jsonToLino({ json: value })` / `linoToJson({ lino })`).
  Rust: `rust/src/protocol.rs` (`LINO_CONTENT_TYPE`, `enum Value {Null,Bool,Int,Float,Str,Array,Object}`).
  C#: `csharp/src/LinksQL/Protocol.cs`, `public static class Protocol` with `Encode`, `Decode`,
  `PrefersJson`, `EncodeReport`, `EncodeIntrospection`, `EncodeSchema`.
- Object encoding convention: `((key value) (key value))`; an array becomes `(a b c)`; an empty object or array becomes `()`.
  Example: `"((operation update) (created ()) ...)"`.

### Query model (worth reusing for the text protocol)
- A query is one or two top-level LiNo nodes: `(restriction)` is a read; `(restriction) (substitution)` is a write.
  The spec's CRUD table:
  ```
  create   ()        ((s t))
  read     ((p))
  update   ((a b))   ((a c))
  delete   ((a b))   ()
  ```
- Stored link canonical form: `(index: source target)`, e.g. `(3: 1 2)`. Indices start at `1`.
- Pattern slots: 2 values gives `{id?, source, target}`. `(x)` with 1 value and no id addresses by identity only.

### QueryReport (§6), the response shape to mirror
```json
{ "operation": "update",
  "matched": [ { "links": [{ "index": 5, "source": 3, "target": 4 }], "binding": {} } ],
  "created": [], "updated": [{ "index": 5, "source": 3, "target": 6 }], "deleted": [] }
```
- `operation` ∈ `read | create | update | delete | mixed | noop` (§6.1). The rule: read-only query → `read`;
  none of created/updated/deleted non-empty → `noop`; exactly one → that one; more than one → `mixed`.
- Error taxonomy (`rust/src/lib.rs:60-90`, `csharp/src/LinksQL/Errors.cs`): `LinoSyntaxError` (carries a
  message and a 0-based `position`, `-1` if none; the message is rendered as `"{message} (at position {n})"`), `LinkIntegrityError`,
  `UnknownNameError`, `SubstitutionError`, `QueryError`, `SchemaError`. These map well to binary error codes.
- Subscriptions (§9) push `{ operation, matching }`. Triggers (§10) have modes `never|once|always`, and link-cli's
  flags of the same names are cited.
- CLI (§12): `linksql serve --port <n> --host <host>`. The spec gives no default port.

Reuse candidates: the `application/lino` content type name, the report field names, the error class names,
the operation classification rule, and the "LiNo first, JSON opt-in" stance.

## 2. lino-rest-api (`/tmp/research/lino-rest-api`, spec `docs/spec/README.md`)
- **HTTP REST**. Packages: JS (`node:http`/Express), Python (ASGI), Rust (axum server, reqwest client).
- Media types (§2): `text/lino` (default, indented), `text/lino-line` (one value per line, used for streaming/NDJSON-like
  bodies), `text/lino-compact` (type-tagged, base64; *"the only form that can carry shared object identity and cycles"*),
  `application/json` (fallback). *"A server MUST include `charset=utf-8` when it emits a LINO media type."*
- Negotiation: `Accept` with q-values, `406`/`415`, `Vary: Accept`. A missing or `*/*` Accept selects `text/lino`.
- Errors: RFC 9457 problem details written in LiNo, served as `text/lino` or `application/problem+lino`:
  ```lino
  ( type "https://link-foundation.github.io/lino-rest-api/errors/not-found"
    title "Not Found"  status 404  detail "Item 42 does not exist"  instance "/items/42" )
  ```
  `type`, `title` and `status` MUST be present. `errors` is reserved for field-level failures.
- Collections: `(items (...) page (limit 20 offset 0 total 2 count 2))`. Also ETag/304/412/428, CORS,
  `/.well-known/lino-api`, OpenAPI 3.1.
- Value model (§3): `null`, `true/false`, int, float, `"quoted"`, `(a b c)`, `(k v)` pairs, via lino-objects-codec.
- Relevance: the `text/lino-line` idea (one message per line) gives a natural newline-delimited framing for a
  text TCP protocol. RFC 9457-in-LiNo is a ready-made error message shape.

## 3. IdDistributorServer / IdClient (C, TCP)
- README: *"Identifier distributor TCP/IP server implementation. A prototype for LinksPlatform cluster's core service."*
- **It has no real message layout.** It is an echo / ping-pong throughput benchmark:
  - `Server.c`: `#define BUFSIZE 32`. Its loop is `recv(sock, buffer, BUFSIZE, 0); send(sock, buffer, BUFSIZE, 0); requestsCount++;`.
  - `Client.c`: `send(*clientSocket, buffer, BUFSIZE, 0); recv(*clientSocket, buffer, BUFSIZE, 0);` (BUFSIZE 32).
  - Messages are fixed 32 bytes with no length prefix, no type byte, and no ID content. No ID counter is implemented.
  - Bug: the server declares `char buffer[8];` but receives `BUFSIZE` (32) bytes, which is a stack overflow (`Server.c:42,68`).
- Host and port are CLI args (`main(argc, argv)`: `hostname = arguments[1]; port = arguments[2]`). No default port.
  `TCP_NODELAY` is set and `BACKLOG 100`. The Linux build uses `select()` (`-DSERVER_SELECT`, `MAX_SOCKETS 1000`); Windows uses Winsock.
- Takeaways: the intent was a minimal, low-latency TCP request/response loop with Nagle disabled. Nothing in it
  can be reused as a format.

## 4. Protocols (`linksplatform/Protocols`, NuGet `Platform.Protocols`, namespace `Platform.Protocols`)
(The issue calls it Platform.Communication. The repo and namespace are now `Protocols` / `Platform.Protocols`.
The `cpp/` folder only has `conanfile.txt`.)
- `Udp/UdpSender.cs`: `class UdpSender : DisposableBase`. Constructors: `(IPEndPoint)`, `(IPAddress, int port)`,
  `(string hostname, int port)`, `(int port)` → loopback. Method: `int Send(string message)`.
- `Udp/UdpReceiver.cs`: `delegate void MessageHandlerCallback(string message)`. `class UdpReceiver : DisposableBase`
  has `private const int DefaultPort = 15000`, constructors `(int listenPort, bool autoStart, MessageHandlerCallback)`,
  `(int, cb)`, `(cb)`, `()`, and members `Start()`, `Stop()`, `string Receive()`, `ReceiveAndHandle()`, `bool Available`.
  It runs a background thread that polls `Available` and else calls `ThreadHelpers.Sleep()`; exceptions are ignored.
- `Udp/UdpClientExtensions.cs`: `DefaultEncoding = Encoding.UTF8`; `SendString(this UdpClient, IPEndPoint, string)`;
  `ReceiveString(this UdpClient)`. One datagram carries one UTF-8 string, with no framing.
- Also contains `Gexf/` (graph XML) and `Xml/Serializer.cs`.
- Takeaways: the API is string-message oriented and uses the callback name `MessageHandlerCallback`. Port 15000 is a historical default.
  A TCP counterpart could mirror `Sender/Receiver(port, handler)`.

## 5. Number representation inside links (Hybrid / external references)

### C# `Platform.Data.Hybrid<T>` (`/tmp/research/Data/csharp/Platform.Data/Hybrid.cs`)
```csharp
public static readonly TLinkAddress HalfOfNumberValuesRange = (NumericType<TLinkAddress>.MaxValue) / TLinkAddress.CreateTruncating(2);
public static readonly TLinkAddress ExternalZero = (HalfOfNumberValuesRange + TLinkAddress.CreateTruncating(1));
public bool IsInternal => SignedValue > 0;
public bool IsExternal => (Value == ExternalZero) || SignedValue < 0;
public long SignedValue => _addressToInt64Converter.Convert(Value);   // UncheckedSignExtendingConverter
public long AbsoluteValue => (Value == ExternalZero) ? 0 : Math.Abs(SignedValue);
public Hybrid(TLinkAddress value, bool isExternal) { if (value == default && isExternal) Value = ExternalZero;
                                                    else Value = isExternal ? -(value) : value; }
public override string ToString() => IsExternal ? $"<{AbsoluteValue}>" : Value.ToString();
```
- So for an N-bit address: `H = 2^(N-1) - 1`, `ExternalZero = 2^(N-1)` (only the MSB set).
- **External = MSB set** (signed interpretation is negative, or the value is exactly ExternalZero).
  The encoding is **two's-complement negation**: raw value `v ≥ 1` is stored as `2^N - v`, and `v = 0` is stored as `2^(N-1)`.
  It is NOT `0x80 | v`. Example (8-bit): raw 1 → `0xFF`, raw 2 → `0xFE`, raw 127 → `0x81`, raw 0 → `0x80`.
- Tests confirm this (`Data/csharp/Platform.Data.Tests/HybridTests.cs`):
  `new Hybrid<byte>(unchecked((byte)128)).AbsoluteValue == 0`, `new Hybrid<byte>(unchecked((byte)-1)).AbsoluteValue == 1`.
  `LinksConstantsTests` uses internal `(1, half)` and external `(half+1, Max)` for byte/ushort/uint/ulong.
- `Numbers/Raw/AddressToRawNumberConverter.cs`: `Convert(source) => new Hybrid<T>(source, isExternal: true)`.
  `Numbers/Raw/RawNumberToAddressConverter.cs`: `Convert(source) => new Hybrid<T>(source).AbsoluteValue`.
  Note the naming: "AddressToRawNumber" means wrapping a plain number into its external (raw) form.

### `LinksConstants<T>` (`Data/csharp/Platform.Data/LinksConstants.cs`)
- With external refs enabled: internal range `(1, H)`, external range `(ExternalZero, MaxValue)`.
- Special constants are taken from the **top of the internal range**: `Continue = H`, `Break = H-1`, `Skip = H-2`,
  `Any = H-3`, `Itself = H-4`, `Error = H-5`. The usable `InternalReferencesRange = (1, H-6)`. `Null = 0`.
  When external refs are disabled, the same constants sit at `MaxValue`, `MaxValue-1`, and so on.

### Resulting ranges (external refs enabled)
| width | internal link refs (raw / usable after 6 constants) | external raw numbers | encoded external values |
|---|---|---|---|
| 8  | 1..127 / 1..121 | 0..127 (128 values) | 0x80..0xFF |
| 16 | 1..32767 / 1..32761 | 0..32767 | 0x8000..0xFFFF |
| 32 | 1..2^31-1 / 1..2^31-7 | 0..2^31-1 | 0x8000_0000..0xFFFF_FFFF |
| 64 | 1..2^63-1 / 1..2^63-7 | 0..2^63-1 | 0x8000…..0xFFFF… |

- Without the hybrid flag, an N-bit unsigned field holds 0..2^N-1. With 0 = null and 1-based indices that is 1..255 for 8-bit.
  The issue's "0-256 → 8-bit" and "0-128 → 8-bit" are each off by one at the boundary.
- Widening: sign extension from N to 2N bits preserves the meaning, **except** ExternalZero. For example, `0x80` must map to `0x8000`,
  because sign-extending would give `0xFF80`, which means raw 128. Narrowing is valid only if the value fits.
- Numbers wider than N-1 bits are chunked: `Data.Doublets.Sequences/.../Numbers/Raw/NumberToLongRawNumberSequenceConverter.cs`
  has `_bitsPerRawNumber = NumericType<TTarget>.BitsSize - 1` and recursively does `Links.GetOrCreate(convertedLowPart, Convert(source >> bitsPerRawNumber))`.
  `BigIntegerToRawNumberSequenceConverter` emits (N-1)-bit chunks, builds a sequence, and marks negatives with `GetOrCreate(NegativeNumberMarker, seq)`.
  `Byte/BytesToRawNumberSequenceConverter` maps each byte to a raw number, then builds a list→sequence.
- Unary numbers (`Numbers/Unary/`): `PowerOf2ToUnaryNumberConverter` sets `2^0 = 1` (link 1) and `2^k = GetOrCreate(2^(k-1), 2^(k-1))`.
  `AddressToUnaryNumberConverter` ORs together the set bits as nested `GetOrCreate(pow2(i), acc)`. Numbers become pure link structure, with no external refs.
- Strings: `CharToUnicodeSymbolConverter` creates `GetOrCreate(AddressToRaw(utf16CodeUnit), UnicodeSymbolType)`.
  `BalancedVariantConverter` pairs neighbours layer by layer into a balanced binary tree of doublets.
  `StringToUnicodeSequenceConverter` creates `GetOrCreate(balancedTree, UnicodeSequenceType)`. Empty string → the type link itself.

### Rust `platform-data` (`/tmp/research/data-rs/src/hybrid.rs`) differs from C#
```rust
pub fn half() -> T { T::MAX / T::from_byte(2) }
fn extend_value(value: T) -> T { (T::MAX - value).wrapping_add(&T::from_byte(1)) }  // = -value
pub fn is_internal(&self) -> bool { self.value < Self::half() }
```
- `external(0)` yields `0`, not ExternalZero. The `default_external` range is `half()..=MAX`, which overlaps the internal
  range `1..=half()` at `half`. Treat it as non-normative. The C# behaviour is the reference.

### This repo's port (link-cli)
- `rust/src/hybrid_reference.rs` matches C# `Hybrid<uint>` exactly (u32 only):
  ```rust
  const EXTERNAL_ZERO: u32 = (u32::MAX / 2) + 1;
  pub fn external(value: u32) -> Self { encoded: if value == 0 { EXTERNAL_ZERO } else { 0u32.wrapping_sub(value) } }
  pub fn absolute_value(self) -> Option<u32> { if encoded == EXTERNAL_ZERO {Some(0)} else if encoded >= EXTERNAL_ZERO {Some(0u32.wrapping_sub(encoded))} else {None} }
  ```
- `rust/src/sequences/`: `AddressToRawNumberConverter::convert(u32) -> external_reference(address)`.
  `RawNumberToAddressConverter::convert` returns `external_reference_value(raw).unwrap_or(raw)` (passes internal values through).
  Also `BalancedVariantConverter`, `CharToUnicodeSymbolConverter` (UTF-16 code units), `StringToUnicodeSequenceConverter`,
  `UnicodeSequenceToStringConverter`, `RightSequenceWalker`, `CachingConverterDecorator`, `TargetMatcher`, `DefaultStack`.
- C#: `csharp/Foundation.Data.Doublets.Cli.Library/NamedLinks.cs` does `new Hybrid<TLinkAddress>(link, isExternal: true)` for
  names of external references. `UnicodeStringStorage.cs` uses `AddressToRawNumberConverter<T>` / `RawNumberToAddressConverter<T>`.
- Dependencies: C# uses `Platform.Data 0.16.1`, `Platform.Data.Doublets 0.18.1`, `Platform.Data.Doublets.Sequences 0.6.5`,
  `Link.Foundation.Links.Notation 0.16.1`. Rust uses `links-notation = "0.16.1"`. Rust `LinkStorage` is `HashMap<u32, Link>`.
- Existing decorator pattern (for new protocol options): `NamedLinksDecorator`, `PinnedTypesDecorator`,
  `NamedTypesDecorator`, `TransactionsDecorator`, `SimpleLinksDecorator`, `VersionControlDecorator`,
  `PersistentTransformationDecorator` (C#). In Rust: `sequences::CachingConverterDecorator`.

## 6. links-notation 0.16.1 API (`/tmp/research/links-notation-0.16`)
### Rust crate `links-notation` (lib name `links_notation`)
- `pub enum LiNo<T> { Link { id: Option<T>, values: Vec<Self> }, Ref(T) }` with `is_ref()`, `is_link()`,
  `LiNo::new(id, values)`, `LiNo::anonymous(values)`, `LiNo::reference(v)`, `format_with_config(&FormatConfig)`,
  and `impl Display`.
- Free functions: `parse_lino(&str) -> Result<LiNo<String>, ParseError>`,
  `parse_lino_to_links(&str) -> Result<Vec<LiNo<String>>, ParseError>`, `format_links(&[LiNo<String>]) -> String`,
  `format_links_with_config(&[LiNo<String>], &FormatConfig) -> String`.
- `LiNoBuilder::new().id(..).value(..).values(..).lino(..).build()`. Also `lino!` macro (re-export from `links-notation-macro`),
  `FormatConfig { less_parentheses, max_line_length, indent_long_lines, max_inline_refs, group_consecutive, indent_string, prefer_inline }`
  with `FormatConfig::builder()`, `parser::Link`, and tuple `From` impls for 2 to 10 elements.
- link-cli uses `use links_notation::{parse_lino_to_links, LiNo};` (`rust/src/parser.rs:8`).
### C# package `Link.Foundation.Links.Notation` (namespace the same)
- Parser: `Parser.peg` (Pegasus), `@classname Parser`, start rule `document <IList<Link<string>>>`. Usage: `new Parser().Parse(text)`
  (link-cli: `BasicQueryProcessor.cs:15-16`, `MixedQueryProcessor.cs:34`).
- `public struct Link<TLinkAddress>` with `Id` and `Values` (an `IList<Link<T>>`). Constructors `(id, values)`, `(values)`, `(params Link[])`, `(id)`.
  `ToString()`, `Simplify()`, `Combine()`, `EscapeReference(string)`, `ToLinkOrIdString()`, and tuple implicit conversions,
  including `(id<T>, source, target)`.
- Formatting: `IListExtensions.Format(this IList<Link<T>>)` (newline-joined), `Format(links, bool lessParentheses)`,
  `Format(links, FormatOptions)`, `LinkFormatExtensions.FormatWithOptions(this Link<T>, FormatOptions)`.
  `FormatOptions { LessParentheses, MaxLineLength=80, IndentLongLines, MaxInlineRefs, ... }`. Also `LinksGroup`, `FormatConfig`.
- Other related libraries: `lino-objects-codec` (NuGet `Lino.Objects.Codec`, crate `lino-objects-codec`; JS `jsonToLino`/`linoToJson`),
  `Data.Doublets.Lino` (C# `LinoImporter`/`LinoExporter`/`ILinoStorage`), and `lino-tokenizer` (*"tokenize Unicode String as sequence of references"*).

## 7. Known wire protocols (V = verified via web fetch on 2026-10-04; K = from model knowledge)
- **PostgreSQL** (V) https://www.postgresql.org/docs/current/protocol-overview.html :
  *"The first byte of a message identifies the message type, and the next four bytes specify the length of the rest of the
  message (this length count includes itself, but not the message-type byte)."* The startup message has no type byte.
  The length is Int32 big-endian. Default port 5432 (K). Message formats: https://www.postgresql.org/docs/current/protocol-message-formats.html
- **MySQL** (V) https://dev.mysql.com/doc/dev/mysql-server/latest/page_protocol_basic_packets.html : header is
  `int<3> payload_length` (little-endian) + `int<1> sequence_id`. Payloads of `2^24-1` bytes or more are split, with `ff ff ff` marking a continuation.
  The sequence id resets at each command. (K) Length-encoded integer (https://dev.mysql.com/doc/dev/mysql-server/latest/page_protocol_basic_dt_integers.html):
  values below 251 take 1 byte; `0xFC` prefix + 2 bytes; `0xFD` + 3 bytes; `0xFE` + 8 bytes (`0xFB` is NULL in rows). This is width escalation by a prefix byte. Port 3306.
- **Redis RESP2/RESP3** (K) https://redis.io/docs/latest/develop/reference/protocol-spec/ : text, CRLF-terminated, and
  the first byte is the type. RESP2 types: `+` simple string, `-` error, `:` integer, `$<len>` bulk string, `*<n>` array.
  RESP3 adds `_` null, `#` bool, `,` double, `(` big number, `!` bulk error, `=` verbatim, `%` map, `~` set,
  `>` push, `|` attribute. `HELLO 3` switches the version. Port 6379. Inline commands are also accepted. This is a good model for a human-typable text mode.
- **SQLite varint** (K) https://www.sqlite.org/fileformat2.html#varint : 1 to 9 bytes, big-endian. Each of the first 8 bytes carries
  7 bits and a high-bit continuation flag, and the 9th byte carries all 8 bits. This lets it reach 64 bits in 9 bytes.
- **LEB128 / protobuf varint** (K) https://protobuf.dev/programming-guides/encoding/ and https://en.wikipedia.org/wiki/LEB128 :
  7 bits per byte, least significant group first, and MSB = continuation. Up to 10 bytes for 64-bit. ZigZag (`(n<<1)^(n>>63)`) is used for signed values.
  Protobuf messages have no self-delimitation, so streams use a varint length prefix (`writeDelimitedTo`).
- **CBOR, RFC 8949** (V) https://www.rfc-editor.org/rfc/rfc8949.html : the initial byte is a 3-bit major type plus a 5-bit additional information field.
  *"Less than 24: The argument's value is the value of the additional information. 24, 25, 26, or 27: The argument's value
  is held in the following 1, 2, 4, or 8 bytes, respectively, in network byte order."* A value of 31 means indefinite length (types 2-5).
  The major types are 0 uint, 1 negative int, 2 bytes, 3 UTF-8 text, 4 array, 5 map, 6 tag, and 7 float/simple. **This is the closest match to the issue's
  8/16/32/64 width scheme.** Major types 0 and 1 also mirror "internal vs external" as two disjoint integer spaces.
  CBOR sequences (RFC 8742) concatenate items with no framing.
- **MessagePack** (K) https://github.com/msgpack/msgpack/blob/master/spec.md : positive fixint `0x00-0x7f`,
  negative fixint `0xe0-0xff`, `uint8/16/32/64 = 0xcc/0xcd/0xce/0xcf`, `int8..64 = 0xd0..0xd3`, fixarray `0x90-0x9f`,
  `array16 0xdc`, `array32 0xdd`, `str8/16/32 0xd9/0xda/0xdb`, `bin8/16/32 0xc4/0xc5/0xc6`. Big-endian.
- **Cap'n Proto** (K) https://capnproto.org/encoding.html : 64-bit words, little-endian. The stream framing is a segment table:
  `uint32 (segmentCount-1)`, then `uint32` size in words for each segment, padded to 8 bytes, then the segments. Pointers are offsets and the format is zero-copy.
- **FlatBuffers** (K) https://flatbuffers.dev/internals/ : little-endian, 32-bit `uoffset_t` offsets, vtables for tables,
  and zero-copy access. There is no built-in stream framing; a size prefix is optional (`FinishSizePrefixed`).
- **Neo4j Bolt / PackStream** (K, graph-DB precedent) https://neo4j.com/docs/bolt/current/ : TCP port 7687. Messages are split into
  chunks, each with a 2-byte big-endian length, and a `00 00` chunk ends the message. The handshake starts with the magic `60 60 B0 17` plus 4 version proposals.
  PackStream integers use `TINY_INT` inline (-16..127), then markers `0xC8/0xC9/0xCA/0xCB` for INT_8/16/32/64. This is again width escalation by a marker.

### Design hints derived from the above

These hints came before the design. The format that was built is specified in
[binary links notation](../../protocol/binary-links-notation.md): the separate
sequence section became sections of any arity.

- Binary frame idea: a magic/version handshake (Bolt), then per message `type byte + length` (PostgreSQL) or a varint length.
- The header can carry `linkCount`, and the reference width follows from it: `count ≤ 2^(w-1)-1` (hybrid) or `≤ 2^w-1` (plain).
  This is equivalent to choosing a CBOR-like 1/2/4/8-byte argument once per message instead of once per value.
- The sequence section's `size` field could be a varint (LEB128) or reuse the per-message width. Sequence ids of
  `count+1, count+2, ...` sit in the same reference space and must be counted when choosing the width.
- Text mode: one LiNo query per line (`text/lino-line` style) or length-prefixed UTF-8. Responses use linksql's QueryReport encoded via
  lino-objects-codec (`((operation ...) (matched ...) (created ...) (updated ...) (deleted ...))`). Errors use
  linksql error names or the RFC 9457-in-LiNo shape.

## 8. links-queue Binary Links Notation (`link-foundation/links-queue` at `bdc7631`)

Found after the design, while looking for a binary notation to share with
[links-notation](https://github.com/link-foundation/links-notation). It came
from links-queue#27 and PR #49. The files are `docs/BINARY-NOTATION-SPEC.md`,
`docs/BINARY-NOTATION-MIGRATION.md`, `js/src/protocol/binary-notation.js` and
`rust/src/backends/binary_notation.rs`.

- **Frame**: 11 bytes: the magic `LNKQ`, a 2-byte version, a flags byte
  (compression, checksum, streaming) and a 4-byte big-endian payload length.
  Then come a LEB128 link count and the links.
- **Link**: a type byte (`SOURCE_IS_ID`, `TARGET_IS_ID`, `SELF_REF`,
  `HAS_ID`, `HAS_VALUES`, id size), an optional LEB128 id, then the source and
  the target. Each is a LEB128 id or a typed literal: null, booleans,
  integers, a float, strings, binary data or an inline link.
- **Transport**: the links-queue TCP server (`rust/src/main.rs`) frames JSON
  queue requests with a 4-byte length. The binary notation is a codec, and the
  JS side offers it in protocol negotiation.

How it compares with [binary links notation](../../protocol/binary-links-notation.md):

| | links-queue | link-cli |
|---|---|---|
| Header | 11 bytes | 1 byte, plus section headers |
| Address of a link | written per link, optional | implicit, from the section; gaps leave holes |
| Reference | LEB128, or a literal | 1, 2, 4 or 8 bytes, fixed per section |
| Numbers and strings | typed literals inside the link | links, or Hybrid external references (§5) |
| Links of other arities | a values array after source and target | sections of any arity range |
| Random access to link *n* | no: every link has a variable size | yes, inside a fixed-arity section |

Sizes of the same stores, measured with
[`evidence/links-queue-sizes/run.sh`](evidence/links-queue-sizes/run.sh)
(the store archive packs both packets with packed widths):

| Store | LiNo | links-queue | store archive |
|---|---|---|---|
| 10 points | 93 | 42 | 26 |
| 1000 points | 14679 | 4759 | 3500 |
| 1000 doublets | 14104 | 6068 | 3414 |

The spec and its implementation disagree on self-references. The spec's
example encodes `(5: 5 5)` as `0F 05` ("source == target, only encode once",
with the id standing for both), but both encoders write `0F 05 05`: the id
once and the shared source and target once more.

Nothing of the layout was reused: a store needs addresses that map one to one
onto the links it holds and fixed-width references, and links-queue's links
are self-describing trees of values. A shared notation would have to cover
both, which is why it is proposed to links-notation rather than adopted here.
