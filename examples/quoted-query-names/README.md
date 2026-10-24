# Quoted numeric query names

Quoted references resolve by name even when their text is a valid link address.
This lets execution records store values such as exit code `"0"` and process id
`"12345"` without their meaning changing as the database grows. Bare numbers
continue to resolve by address.

The script repeats issue #130's queries, checks execution record values, and
replays the exported LiNo as a strict query into a fresh database. It uses a
temporary directory and removes it on exit.

From the repository root, run it against either implementation after building:

```sh
dotnet build csharp --configuration Release
python3 examples/quoted-query-names/run.py dotnet csharp/Foundation.Data.Doublets.Cli/bin/Release/net10.0/clink.dll

cargo build --manifest-path rust/Cargo.toml
python3 examples/quoted-query-names/run.py rust/target/debug/clink
```
