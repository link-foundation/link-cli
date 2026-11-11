# Serving and relaying links over every IPC mechanism

The Rust and C# `clink` serve the same store on any endpoint of
[IPC transports v1](../../docs/protocol/ipc-transports-v1.md): TCP and UDP
loopback, Unix domain sockets, named and anonymous pipes, FIFOs, shared
memory, memory-mapped files, files on disk, OS signals, POSIX and System V
message queues, Windows mailslots, `WM_COPYDATA` and ALPC. Every endpoint
carries both the text and the binary LiNo protocol, and the transports are
decorators: a server, a client and a relay take any of them and stack.

Serve one store on several endpoints at once (repeat `--serve`):

```sh
clink --db example.links --serve 127.0.0.1:7777 --serve shm:/tmp/example-links --serve unix:/tmp/example.sock
```

Talk to it over any of them, in either protocol, from either implementation:

```sh
clink --connect shm:/tmp/example-links '() ((1 1))'
clink --connect unix:/tmp/example.sock --protocol binary ''
```

Stack another transport with a relay: `--forward` relays every connection of
its `--serve` endpoints, byte for byte, without opening a database. The Rust
and C# relays mix freely with either server:

```sh
clink --serve mq:/tmp/example-queue --forward shm:/tmp/example-links
clink --connect mq:/tmp/example-queue ''
```

The anonymous pipes of `stdio` need no listener at all: `exec:` starts the
server as a child and talks through its standard input and output:

```sh
clink --connect 'exec:clink --db example.links --serve stdio' ''
```

`--trace` (or `CLINK_IPC_TRACE=1`) logs each transport event to standard
error. On Windows use `pipe:NAME` for a named pipe and fresh absolute
directories such as `mailslot:C:\Temp\example-mail`, `copydata:C:\Temp\example-copy`
or `alpc:C:\Temp\example-alpc` for the Windows-only kinds.

## JavaScript

The JS client connects to every endpoint too: natively where Node.js has the
mechanism, through the `clink` bridge (`CLINK_BIN`) otherwise. Against an
empty store:

```sh
node examples/ipc/javascript-client.mjs shm:/tmp/example-links
CLINK_BIN=$(command -v clink) node examples/ipc/javascript-client.mjs mq:/tmp/example-queue
```

## Every kind between every implementation

[`interop.mjs`](interop.mjs) runs every endpoint kind of the platform between
every pair of implementations: each serves, each connects in both protocols,
each relays to the other and each reaches a `stdio` server through `exec:`.
The [IPC workflow](../../.github/workflows/ipc.yml) runs it on Linux, macOS and
Windows:

```sh
cargo build --release --manifest-path rust/Cargo.toml --bin clink
dotnet build -c Release csharp/Foundation.Data.Doublets.Cli/Foundation.Data.Doublets.Cli.csproj
node examples/ipc/interop.mjs rust=rust/target/release/clink \
  'csharp=["dotnet","csharp/Foundation.Data.Doublets.Cli/bin/Release/net10.0/clink.dll"]'
```

`--kinds shm,mq` limits the kinds; the script exits with 1 when any
conversation fails.
