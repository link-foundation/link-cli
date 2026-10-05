---
'Foundation.Data.Doublets.Cli': patch
---

`LinksPacket.AddressTier` now throws an unencodable `LinoProtocolException` for an address beyond the internal range instead of silently returning width 8, and the unused `LinoFormat.IsReference` was removed. A server that is stopping now answers a request it already read with `(error: 'server is shutting down')` instead of hanging up, and its trace notes when a client hangs up. `LinksPacket.WidthFromCode` is total over two-bit codes, and the accept loop no longer keeps a separate branch for a client accepted while stopping (its worker closes it). Tests now cover every line of the protocol code, and each malformed-packet test asserts the exact error, which fixed three cases that passed for the wrong reason.
