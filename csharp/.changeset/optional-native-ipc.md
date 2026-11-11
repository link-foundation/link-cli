---
'Foundation.Data.Doublets.Cli': minor
---

Carry the LiNo text and binary protocols over every IPC mechanism of
IPC transports v1 (`docs/protocol/ipc-transports-v1.md`) (#140): TCP and UDP
loopback, Unix domain sockets, Windows named pipes, POSIX FIFOs, anonymous pipes
(`stdio` and `exec:`), shared memory, memory-mapped files, files on disk, OS
signals, POSIX and System V message queues, mailslots, `WM_COPYDATA` and ALPC.
`Foundation.Data.Doublets.Cli.Ipc` selects the mechanism by an endpoint string
(`IpcTransports.Connect`, `Listen`, `ListenAll`, `Relay`, `Register`),
`LinksServer.ListenAll` and `LinksClient.ConnectEndpoint` take any endpoint, and
the transports interoperate with the Rust `clink` and the JavaScript client.
`clink --serve` repeats to serve one store on several endpoints, `--forward`
relays every connection to another endpoint without opening a database so
transports stack, `--connect` takes any endpoint and `--trace` (or
`CLINK_IPC_TRACE=1`) logs every transport event. System.CommandLine is 3.0.0
and the tests use Microsoft.NET.Test.Sdk 18.12.0.
