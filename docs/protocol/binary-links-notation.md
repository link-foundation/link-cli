# Binary links notation, version 1

Binary links notation stores **links**, each a tuple of one or more
references, in a self-delimiting packet. Both ports use the implementation
that [links-notation](https://github.com/link-foundation/links-notation) 0.23.0
ships: `links_notation::binary` in Rust and
`Link.Foundation.Links.Notation.Binary` in C# (issue #104). link-cli adds only
the transport around it: text framing, protocol detection (§2), the server,
the client and the store archive (§10).
[`binary-links-notation-vectors.txt`](binary-links-notation-vectors.txt)
holds golden vectors. Both test suites check them, so both ports write and
read exactly these bytes.

The packet layer knows nothing about LiNo. The binary LiNo protocol (§7) is
one user of it, and the store archive (§10), a dump of a whole links store,
is another.

## 1. Conventions

- **LEB128** means unsigned LEB128: 7 bits per byte, least significant group
  first, and the high bit set on every byte except the last. A value must fit
  in 64 bits.
- A **reference** is `width` bytes, little-endian, where `width` is 1, 2, 4
  or 8. The width is set per section (§3).
- An **address** is a link's position. `0` is null, and links are numbered
  from `1`.

## 2. Header byte

```text
bit  7 6 5 4   3 2      1          0
     0 0 0 1   width    explicit   external
     version   (log2)   layout     references
```

- **Version** is the high nibble. Version 1 is `0x1_`. A text LiNo message
  never starts with a byte in `0x10..=0x1F`, so the header also tells a
  binary packet apart from a text message. `0x2_` to `0xF_` are reserved.
- **External references** (bit 0): the top bit of every reference marks an
  external value (§5).
- **Explicit layout** (bit 1): section headers follow (§3.2). Otherwise the
  packet uses the compact layout (§3.1).
- **Width** (bits 2–3): the log2 of the reference width in the compact
  layout. The explicit layout must keep these bits clear.

## 3. Layouts and sections

A packet is a list of **sections**. A section has:

- a **gap**: how many addresses are skipped before it;
- an **arity range** `min..=max`: how many references each of its links holds;
- a **width**;
- its links.

The first section starts at address `1 + gap`, and every later section starts
at `previous end + gap`. So gaps leave holes in the address space, and a
packet can hold any ascending set of addresses.

### 3.1 Compact layout

```text
0x10 | external | width << 2
LEB128 N                       the number of links, all doublets
```

The compact layout is exactly one section with gap 5, arity 2 and the header
width. That is the common case of a LiNo document made of doublets, and it
costs just two header bytes. An empty packet is `10 00`, or `11 00` with
external references.

### 3.2 Explicit layout

```text
0x12 | external
LEB128 S                       the number of sections, then S section headers:
  LEB128 shape                 bits 0-1  log2 of the reference width
                               bit 2     a gap follows
                               bit 3     variable arity
                               bits 4+   min, at least 1
  LEB128 gap                   only when bit 2 is set; a gap of 0 is not written
  LEB128 extra                 only when bit 3 is set: 0 = no maximum, else max - min
  LEB128 count                 the number of links in the section
```

A section without the variable-arity bit has a fixed arity: every link holds
exactly `min` references.

### 3.3 Links

After the headers come the links, section by section and in address order.

- A link in a **fixed-arity** section is `min` references.
- A link in a **variable-arity** section is `LEB128 (length - min)` followed
  by `length` references. The length must lie in the section's range.

Each reference takes the width of its section.

### 3.4 Examples

These packets come from the `links` vectors. A section is written as
`address:references`, and `#v` is the external value `v`.

`1:1 1; 2:1 2 3; 3:2 2 2; 10:3 9` becomes
`12 03 20 01 30 02 24 06 01 01 01 01 02 03 02 02 02 03 09`:

| Bytes | Meaning |
|---|---|
| `12` | version 1, explicit layout |
| `03` | three sections |
| `20 01` | shape: min 2, fixed, 1-byte references; 1 link (address 1) |
| `30 02` | min 3, fixed, 1 byte; 2 links (addresses 2 and 3) |
| `24 06 01` | min 2, gap 6, 1 byte; 1 link at `3 + 1 + 6 = 10` |
| `01 01` | link 1 |
| `01 02 03` `02 02 02` | links 2 and 3 |
| `03 09` | link 10 |

`1:1; 2:1 1; 3:1 1 1; 4:1 1 1 1` becomes
`12 01 18 03 04 00 01 01 01 01 02 01 01 01 03 01 01 01 01`. The shape `18` is
min 1, variable arity and 1-byte references. `03` is the extra (max 4) and
`04` is the count. Every link starts with its length minus 1.

## 4. Widths

An internal reference `a` needs the narrowest width that holds `a`:

| Width | Plain addresses | With external references |
|---|---|---|
| 1 byte | `0..=255` | `0..=127` |
| 2 bytes | `0..=65 535` | `0..=32 767` |
| 4 bytes | `0..=2³²−1` | `0..=2³¹−1` |
| 8 bytes | `0..=2⁶⁴−1` | `0..=2⁶³−1` |

A section's width must hold every reference in it. A link never needs to be
wide because of its own address, only because of the references it holds.

## 5. External references

With bit 0 of the header set, a reference is either a link address or an
**external value**, exactly like `Platform.Data.Hybrid<T>`. At a width of
`w` bits:

- an external value `v ≥ 1` is stored as `2^w − v`, which is its
  two's-complement negation;
- an external `0` is stored as `2^(w−1)`;
- any raw value of `2^(w−1)` or more is external, and anything below it is an
  internal address.

An external value up to `2^(w−1) − 1` fits `w` bits, so it can be at most
2⁶³ − 1. Without external references, every reference is an internal address.

## 6. Packing

`pack(external_references, links, packed_widths)` lays out
`(address, references)` pairs given in ascending address order:

- A hole between two addresses always starts a new section.
- Links of one length can share a fixed-arity section. Links of differing
  lengths need a variable-arity section or separate sections.
- **Uniform widths** (the default) give every section the width of the widest
  reference in the packet, so all references have one size.
- **Packed widths** let each section take the narrowest width its own links
  need, and split sections wherever that saves bytes.

A linear dynamic program chooses the sections. Its state is the width and
whether the arity is fixed or variable, which makes eight states. A link
either continues the section of the previous link or opens a new one, at an
estimated cost of 2 bytes for a section header and 1 more byte for a
variable arity. With packed widths, `pack` also builds the uniform layout and
keeps the packed one only when it is strictly smaller. So **packed widths
never cost more than uniform ones.** Both ports break ties the same way, so
they produce identical bytes.

A packet whose only section matches §3.1 is written in the compact layout.

## 7. LiNo mapping

The binary LiNo protocol maps a LiNo document to links and packs them. It
uses only links, the way linksplatform represents data. Addresses `1` to `5`
are fixed **marker points** that are never transmitted, and links start at
`6`:

| Address | Meaning |
|---|---|
| `0` | null, the empty link `()` |
| `1` | `One`, the unary 1 |
| `2` | `Number`: `(Number unary)` is a non-negative integer |
| `3` | `String`: `(String code-points…)` is a Unicode string |
| `4` | `List`: `(List elements…)` is a list of links |
| `5` | `Identified`: `(Identified id values…)` is a link with an id |
| `6…` | the links of the document |

| LiNo | Links |
|---|---|
| `()` | `0` |
| number `n` | `(Number unary(n))`. `unary(0)` is null, `2^0` is `One`, `2^k` is `(2^(k-1) 2^(k-1))`, and other numbers are right-nested sums of powers of two from the highest bit down. With external references, `n` is the external value `n` instead |
| any other reference | `(String code-point…)`. Each code point is a unary number, or an external value with external references |
| a link of two values, no id | a doublet |
| a link of any other number of values, no id | a list |
| a link with an id | `(Identified id value…)` |
| the document | the list of its top-level links, stored last as the root |

A link of two values is always a doublet. A list or typed value of any
other length `n` becomes **one link** of `n` references when the arity range
contains `n`. That link is the bare elements for a list, or
`marker elements…` for a typed value. Otherwise it becomes the doublet
`(marker chain)`, where `chain` is the nil-terminated cons list
`(e1 (e2 (… (en 0))))`. Identical sub-links are written once and shared,
because links are content-addressed. Links are numbered so that each one
refers only to earlier ones: the doublets of plain doublets come first, then
the rest in creation order.

### 7.1 Options

Each option is off by default and can be switched on on its own:

| Option | Rust | C# | CLI | Effect |
|---|---|---|---|---|
| external references | `with_external_references(true)` | `WithExternalReferences()` | `--external-references` | numbers and code points become external values (§5) |
| arity | `with_arity(ArityRange)` | `WithArity(ArityRange)` | `--arity 2..3` | the link lengths the encoder may use. The default, `2`, writes only doublets. `2..3` adds triplets, and `1..` allows any length. The range must contain 2 |
| packed widths | `with_packed_widths(true)` | `WithPackedWidths()` | `--packed-widths` | per-section widths (§6) |

An arity is written `n`, `min..max` (inclusive) or `min..` (no maximum).

A decoder needs none of these options. The header and the section headers
announce everything. A server answers in the style of the request: the
reply options come from the received packet. External references are taken
from the header. The arity is `min(2, shortest)..=max(2, longest)` over the
link lengths in the packet, and packed widths are on when the sections
differ in width.

### 7.2 Example

`() ((1 1))` with the defaults becomes
`10 07 02 01 06 06 07 00 04 08 00 09 0a 00 04 0b`, which is the compact layout
with 7 doublets of 1 byte each:

| Address | Link | Meaning |
|---|---|---|
| 6 | `02 01` | `(Number One)`, the number 1 |
| 7 | `06 06` | `(1 1)` |
| 8 | `07 00` | the chain `((1 1))` |
| 9 | `04 08` | `(List 8)`, the list `((1 1))` |
| 10 | `00 09` | `() ((1 1))` |
| 11 | `0a 00` | the chain of top-level links |
| 12 | `04 0b` | `(List 11)`, the document |

With `--arity 1..`, the two lists become single links and the packet becomes
`12 04 24 05 02 10 01 20 01 10 01 02 01 06 06 07 00 08 09`. That is four
fixed-arity sections: doublets 6 and 7 after the gap of 5, the one-element
list `8 = (7)`, the doublet `9 = (0 8)` and the document `10 = (9)`.

## 8. Decoding untrusted input

A packet must be read whole. Bytes after it, or a packet that ends early,
are an error. The decoder rejects:

- a header outside `0x10..=0x1F`, or an explicit layout with width bits set;
- a LEB128 value over 64 bits;
- a section shape with min 0, or an arity range that overflows;
- a link length outside its section's arity;
- addresses that overflow 64 bits.

`DecodeLimits` bounds the work a peer can cause, and the encoder checks the
same limits, so a protocol never writes a packet its peer would reject:

| Limit | Default |
|---|---|
| links in a packet | 2²² |
| references in all links | 2²⁴ |
| LiNo nodes a packet expands to | 2²² |
| bytes of all strings a packet expands to | 64 MiB |
| LiNo nesting depth | 64 |

The text protocol has its own limit, 64 MiB per message
(`TextLinoProtocol::max_text_bytes` in Rust, `TextLinoProtocol.MaxTextBytes`
in C#). `ProtocolLimits` carries both budgets for the server and for
protocol detection.

The LiNo mapping also requires that:

- links are contiguous from address 6;
- each link refers only to earlier links;
- markers appear only in marker position;
- numbers are well-formed unary values and code points are valid Unicode
  scalar values;
- the root is a list.

## 9. Sizes

The table below lists the number of bytes each encoding of a document takes.
The values come from
`cargo run --example lino_binary_dump -- '<document>'` in `rust/`. The
text size includes the `\n.\n` terminator. When the packed size differs from
the uniform size, it is given after a slash.

| Document | text | plain 2 | plain 2..3 | plain 1.. | ext 2 | ext 2..3 | ext 1.. |
|---|---|---|---|---|---|---|---|
| `() ((1 1))` | 13 | 16 | 16 | 19 | 14 | 14 | 16 |
| `((1: 1 1)) ((1: 1 2))` | 24 | 38 | 38 | 33 | 32 | 32 | 25 |
| `() ((child: father mother))` | 30 | 132 | 132 | 123 | 52 | 52 | 41 |
| `() ((1000 70000))` | 20 | 70 | 70 | 73 | 50 / 25 | 50 / 25 | 34 / 23 |
| `(a b c d e f g h)` | 20 | 108 | 108 | 103 | 56 | 56 | 51 |

The plain mapping is not meant to beat text. It sends a document as nothing
but links, which a links store can take in without a parser. External
references, a wider arity and packed widths are the options that make a
packet small. Packed widths matter when a few large values would otherwise
widen every reference: in `() ((1000 70000))`, only the link holding 70 000
needs 4 bytes.

## 10. Store archive

A store archive holds a whole doublets store, names included, as two packets
written one after the other:

1. **Links**, without external references and with packed widths. The store
   link `(a: s t)` is the doublet `s t` at address `a`. Addresses the store
   does not use are holes (section gaps), so every link keeps its address.
2. **Names**, with external references and with packed widths. One link per
   named store link, in address order and from address 1, holds the external
   values `address code-point…`. An empty name is a link of arity 1.

An empty store is `10 00 11 00`.

Importing reads both packets without decode limits and rejects:

- an archive that is cut short or has bytes after its second packet;
- a link that is not a doublet of internal references;
- an address that does not fit a 32-bit store;
- a name that holds an internal reference or an invalid code point.

Only then does it touch the store: it creates every missing address, writes
each link at its own address, and sets every name. So exporting the imported
store gives the same bytes again.

The golden archive of `rust/tests/store_archive_tests.rs` and
`StoreArchiveTests.cs` holds `(a: a a)`, `(é: é é)` at address 3 and
`(ab: a é)` at address 4, with a hole at 2. It is
`12 02 20 01 24 01 02 01 01 03 03 01 03 13 02 21 02 30 01 ff ff 9f ff fd ff 17 ff fc 9f 9e`:

| Bytes | Meaning |
|---|---|
| `12 02` | links packet: explicit layout, two sections |
| `20 01` | min 2, 1-byte references; 1 link (address 1) |
| `24 01 02` | min 2, gap 1, 1 byte; 2 links (addresses 3 and 4) |
| `01 01` `03 03` `01 03` | links 1, 3 and 4 |
| `13 02` | names packet: explicit layout, external references, two sections |
| `21 02` | min 2, 2-byte references; 2 links |
| `30 01` | min 3, 1-byte references; 1 link |
| `ff ff` `9f ff` | `#1 #97`: link 1 is `a` |
| `fd ff` `17 ff` | `#3 #233`: link 3 is `é` |
| `fc 9f 9e` | `#4 #97 #98`: link 4 is `ab` |

The CLI writes an archive with `--export-binary` (or `--binary-output`,
`--binary-out`) wherever it writes `--out`, and reads one with
`--import-binary` (or `--binary-input`, `--binary-in`) before `--in` and the
query. The libraries expose the same operations as `export_store`,
`export_store_file`, `import_store` and `import_store_file` in Rust and
`StoreArchive.Export`, `ExportToFile`, `Import` and `ImportFromFile` in C#.
[`examples/archive`](../../examples/archive/README.md) copies a store from one
port to the other through an archive.
