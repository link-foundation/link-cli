---
'Foundation.Data.Doublets.Cli': minor
---

Added LiNo substitution operations over TCP (issue #105). There are two protocols, and either can replace the other:

- `TextLinoProtocol`: dot-stuffed UTF-8 messages.
- `BinaryLinoProtocol`: each packet is a list of sections of links that share one reference width (8, 16, 32 or 64 bits) and one arity range. The default is doublets with one uniform width. Options add external (Hybrid) references, any arity range such as `2..3` or `1..` (`ArityRange`), and packed per-section widths; packed output is never larger than uniform output. Shared golden vectors in `docs/protocol/binary-links-notation-vectors.txt` pin the format.

`LinksServer` and `LinksClient` run create, read, update and delete queries over either protocol. The new `clink --serve` and `--connect` options do the same from the command line, together with `--protocol`, `--external-references`, `--arity` and `--packed-widths`. The wire format is byte-for-byte the same as the Rust port's, so C# and Rust servers and clients work with each other.

Upgraded Link.Foundation.Links.Notation to 0.22.0. In earlier versions, parse time grew exponentially with nesting depth, so one short message could stall a server.
