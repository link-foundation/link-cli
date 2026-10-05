# Store archive interop

`run-interop.sh` builds both ports, fills a store with one of them, writes it
with `--export-binary` and reads it back with the other port's
`--import-binary`. The two stores must print the same LiNo, so the script ends
with an empty `diff`:

```bash
./examples/archive/run-interop.sh            # Rust exports, C# imports
./examples/archive/run-interop.sh csharp     # C# exports, Rust imports
```

Both directions print the same 41-byte archive for a store whose LiNo export
is 80 bytes:

```
archive: 41 bytes, LiNo: 80 bytes
 12 02 20 03 24 01 01 01 01 02 02 01 02 05 05 13
 02 70 02 60 01 ff 9a 9f 8c 98 9b 8e fe 93 91 8c
 98 9b 8e fd 9d 98 97 94 9c
(father: father father)
(mother: mother mother)
(child: father mother)
(5: 5 5)
```

The first 15 bytes are the links packet: links 1 to 3, then a gap of one
address (the hole at 4) and link 5. The rest is the names packet: `father`
and `mother` (seven references each), then `child` (six). The format is
described in
[docs/protocol/binary-links-notation.md](../../docs/protocol/binary-links-notation.md#10-store-archive).
