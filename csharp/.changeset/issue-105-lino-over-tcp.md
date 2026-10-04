---
'Foundation.Data.Doublets.Cli': minor
---

Added LiNo substitution operations over TCP (issue #105). There are two protocols, and either can replace the other:

- `TextLinoProtocol`: dot-stuffed UTF-8 messages.
- `BinaryLinoProtocol`: each packet carries its link count, and reference widths grow from 8 to 16, 32 and 64 bits. Optional decorators add external (Hybrid) references, a variable-length sequence section and progressive widths.

`LinksServer` and `LinksClient` run create, read, update and delete queries over either protocol. The new `clink --serve` and `--connect` options do the same from the command line, together with `--protocol`, `--external-references`, `--sequences` and `--progressive-widths`. The wire format is byte-for-byte the same as the Rust port's, so C# and Rust servers and clients work with each other.

Upgraded Link.Foundation.Links.Notation to 0.22.0. In earlier versions, parse time grew exponentially with nesting depth, so one short message could stall a server.
