---
'Foundation.Data.Doublets.Cli': major
---

The binary links notation codec now comes from `Link.Foundation.Links.Notation.Binary` (links-notation 0.23.0) instead of a local copy. `ArityRange`, `LinksPacket`, `Section`, `PacketReference`, `DecodeLimits`, `LinoMapping` and `LinoFormat` are removed from `Foundation.Data.Doublets.Cli.Protocol`; import them from `Link.Foundation.Links.Notation.Binary`. `LinoStreamReader` is replaced by `PacketReader`.

- `DecodeLimits.MaxTextBytes` moves to `TextLinoProtocol.MaxTextBytes`. The new `ProtocolLimits` record (`Binary`, `MaxTextBytes`, `Default`, `Unlimited`) is now the type taken by `LinksServerOptions.Limits`, `MessageFormat.Protocol` and `LinoProtocols.ReadAnyDocument`.
- `BinaryLinoProtocol` now enforces `Limits` when it encodes too. The upstream defaults apply, so the default `MaxDepth` is 64 and there is a new `MaxStringBytes` limit.
- The protocols still raise only `LinoProtocolException`. A codec `BinaryNotationException` is converted with the same kind and detail and is kept as `InnerException`, and `LinoProtocolException.From` exposes this conversion.
