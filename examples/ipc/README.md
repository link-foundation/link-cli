# A client and server with optional IPC layers

Install the adapter from the repository root:

```sh
python -m pip install ./ipc
```

Start a Rust or C# `clink` database server on `127.0.0.1:7777`. In a second
terminal, expose it through native shared memory:

```sh
clink-ipc --listen shm:/tmp/example-links --connect tcp:127.0.0.1:7777
```

In a third terminal, add a client-side layer:

```sh
clink-ipc --listen tcp:127.0.0.1:7778 --connect shm:/tmp/example-links
```

Any existing Rust, C# or JS client can now connect to `127.0.0.1:7778`:

With an empty database, the existing JavaScript client can run the automated
example with `node examples/ipc/javascript-client.mjs 127.0.0.1:7778`.
It creates, reads and deletes a link through the stack. CLI examples:

```sh
clink --connect 127.0.0.1:7778 --protocol text '(create: (0 0))'
clink --connect 127.0.0.1:7778 --protocol binary '(count: ())'
```

To add an anonymous-pipe hop, replace the third terminal's command with:

```sh
clink-ipc --listen tcp:127.0.0.1:7778 --connect 'exec:["clink-ipc","--listen","stdio:","--connect","shm:/tmp/example-links"]'
```

The bytes between the last two adapters now use inherited stdin/stdout pipes;
the next hop still uses shared memory. `--trace` logs byte counts to stderr.
Stop the adapters with Ctrl+C after closing the clients. On Windows use fresh
absolute directories such as `shm:C:\Temp\example-links` consistently at both
ends, and pass the JSON array with the quoting rules of your shell.

The [configuration guide](../../ipc/README.md) lists every endpoint, limits
and the Python API. Automated runnable counterparts live in
[the IPC protocol tests](../../ipc/tests/test_protocols.py).
