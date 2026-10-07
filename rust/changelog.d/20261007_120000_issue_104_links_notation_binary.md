---
bump: major
---

The binary links notation now comes from `links_notation::binary` (links-notation 0.23.0) instead of a copy in `link_cli::protocol` (issue #104). `link_cli::protocol` re-exports it, so `protocol::packet`, `LinksPacket`, `ArityRange`, `BinaryLinoOptions`, `DecodeLimits`, `encode_document`, `decode_document`, `format_document` and `parse_document` keep their paths. Breaking changes:

- `DecodeLimits::max_text_bytes` is gone: the text limit belongs to the transport, so `TextLinoProtocol { limits }` becomes `TextLinoProtocol { max_text_bytes }` (64 MiB by default). The new `ProtocolLimits { binary, max_text_bytes }` replaces `DecodeLimits` in `ServerOptions::limits`, `read_any_document` and `MessageFormat::protocol`.
- The upstream defaults apply: the default `max_depth` drops from 1024 to 64, the same depth the text parser accepts, and the new `max_string_bytes` limit (64 MiB) bounds the strings a packet expands to.
- `BinaryLinoProtocol` checks its limits when it encodes too, so it fails with `ProtocolError::Unencodable` instead of writing a packet its peer would reject.
- The re-exported codec (`encode_document`, `decode_document`, `parse_document`, `LinksPacket::from_bytes`, `LinksPacket::read_from` and the rest of `protocol::packet`) returns `BinaryError` instead of `ProtocolError`. Every protocol, server, client and archive function still returns `ProtocolError`, and `From<BinaryError> for ProtocolError` keeps the kind and message, so `?` keeps working.
- The upstream decoder validates more: a packet's link count and references are checked against the limits before any link is decoded.

Updated `links-notation` to 0.23.0. The `Dependencies` workflow now checks every manifest git tracks, and a dependency can be held back only by a comment on its manifest line that links an open issue.
