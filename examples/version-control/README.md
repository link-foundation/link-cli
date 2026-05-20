# Version-control layer — examples

This folder contains small, runnable demonstrations of the optional
**version-control** decorator added in issue
[#94](https://github.com/link-foundation/link-cli/issues/94). The
decorator sits *above* the transactions layer and adds time travel,
branching, and tagging over the recorded transitions log. Both the
C# and the Rust CLIs expose the same flag surface.

| Binary | Invocation |
|--------|------------|
| C#     | `dotnet run --project csharp/Foundation.Data.Doublets.Cli --` |
| Rust   | `cargo run --manifest-path rust/Cargo.toml --` |

## Scripts

| File | What it shows |
|------|---------------|
| `run-csharp.sh` | End-to-end version-control demo using the C# `clink` binary |
| `run-rust.sh`   | End-to-end version-control demo using the Rust `clink` binary |
| `README.md`     | This file |

## What the demo does

1. Creates a few links on the default `main` branch.
2. Tags the current head as `v1`.
3. Forks a new branch `feature` from the current head, applies more
   changes there, then tags `v2`.
4. Switches back to `main` — the `feature`-only transitions are rolled
   back so the data store again matches `main`.
5. `--checkout v1` rewinds the live store further to the `v1` tag.
6. Lists branches and tags.

## Key flags

```text
--vc                         Enable the version-control decorator
                              (implies --transactions)
--vc-file <file>             Explicit version-control store path
                              (default: <db>.versioncontrol.links)
--branch <name>              Switch to branch, create it if --branch-from
                              is also passed. Implies --vc.
--branch-from <seq>          Fork point when creating a branch
--checkout <seq|tag>         Time-travel to a sequence number or named tag
                              Implies --vc.
--tag <name[=seq]>           Tag the current head, or a specific seq.
                              Implies --vc.
--list-branches              Print all branches and exit
--list-tags                  Print all tags and exit
```

When neither `--vc` nor any version-control flag is used the bare CLI
behaviour is unchanged: no `versioncontrol.links` sidecar is created
(R17).
