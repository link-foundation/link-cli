# Transactions layer — examples

This folder contains small, runnable demonstrations of the optional
**transactions** decorator added in issue
[#94](https://github.com/link-foundation/link-cli/issues/94). Both the
C# and the Rust CLIs expose the same flag surface, so the same shell
script works for either binary — pick the one you have installed:

| Binary | Invocation |
|--------|------------|
| C#     | `dotnet run --project csharp/Foundation.Data.Doublets.Cli --` |
| Rust   | `cargo run --manifest-path rust/Cargo.toml --` |

## Scripts

| File | What it shows |
|------|---------------|
| `run-csharp.sh` | End-to-end transactions demo using the C# `clink` binary |
| `run-rust.sh`   | End-to-end transactions demo using the Rust `clink` binary |
| `README.md`     | This file |

## What the demo does

1. Creates two links inside an explicit `--transactions` session — the
   writes go to `data.links` and a side-car transitions log is written
   to `data.transitions.links`.
2. Prints the resulting transitions log with `--log`, so you can see
   the `Create / Update / Delete` records, their sequence numbers, the
   transaction ids that grouped them, and the (index, source, target)
   before/after states.
3. Demonstrates each commit mode (`--commit-mode sync` and
   `--commit-mode async`) and each retention policy (`--retention
   infinite`, `--retention sized:N`, `--retention chunked:N:/path`).

## Key flags

```text
--transactions               Enable the transactions decorator
--transactions-file <file>   Explicit transitions log path (implies --transactions)
--commit-mode <sync|async>   Commit mode (implies --transactions)
--retention <spec>           Retention policy (implies --transactions)
                              spec ∈ { infinite | sized:N | chunked:N:DIR }
--log                        Print transitions log and exit (implies --transactions)
```

When *no* transaction flag is used the bare `clink` behaviour is
unchanged: no transitions file is written and no extra runtime cost is
paid (R8 / R9 / R17 from the requirements doc).
