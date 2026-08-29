# Issue 100 / PR 101 verification logs

Captured on the final commit of `issue-100-f2e0ccb162ad`, so the numbers quoted
in [the case study](../../../../../../../docs/case-studies/issue-100/README.md#8-verification)
can be checked against the runs that produced them.

| File | Command | Result |
|------|---------|--------|
| `rust-tests.txt` | `cargo test` (in `rust/`) | 239 passed, 0 failed, 1 ignored |
| `csharp-format-build-test.txt` | `dotnet format --verify-no-changes`, `dotnet build -c Release`, `dotnet test` (in `csharp/`) | format clean, build 0 warnings / 0 errors, 254 passed |
| `cli-parity.txt` | `docs/case-studies/issue-100/evidence/cli-parity/run.sh` | 39 scenarios agree, 1 known upstream difference |
| `js-tests.txt` | `node --test test/*.mjs` (in `js/`) | 9 passed |

The `FORMAT=`, `BUILD=` and `TEST=` lines in the C# log are the exit statuses of
the three commands, in order.

`cli-parity.txt` ends in `KNOWN update into duplicate`, which is the
`Platform.Data.Doublets` 0.18.1 `MergeUsages` defect
([Data.Doublets#515](https://github.com/linksplatform/Data.Doublets/issues/515)),
not a failure — see the case study, §5.5.
