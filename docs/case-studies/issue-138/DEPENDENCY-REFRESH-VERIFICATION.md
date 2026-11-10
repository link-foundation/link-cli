# Registry refresh after recovered protocol work

The Dependencies workflow on `4160136b13cfb2588c65e79a324dde64b618056e`
failed at 2026-10-28T11:49:51Z, after the earlier passing head. The
[run](https://github.com/link-foundation/link-cli/actions/runs/40682344781)
checked 127 versions and all Cargo lockfiles. Its complete downloaded log is
`dev/log/issues/138/pulls/139/dependencies-40682344781.log`: lines 198–206
identify doublets 0.8.0 versus 0.9.0 in both Rust manifests, and lucide-react
1.62.0 versus 1.63.0 in both npm files. The following `e6fa2af` run repeats
the failure; its complete log is retained alongside it. Neither failure
establishes a query-protocol regression.

Update the two Rust manifests, all three Rust lockfiles, and the npm manifest
and lockfile. The published doublets 0.8.0/0.9.0 production-source diff changes
only `src/mem/split/store.rs`; public signatures and dependencies are unchanged.
It fixes concrete target counts for absent sources and migrates forward
references from external indexes when their referenced IDs are allocated.
The underlying platform-mem version remains 0.3.0; its open generic mapped
failure-safety criteria remain open.

`rust/tests/doublets_split_index_tests.rs` contains four finite regressions for
counts, source migration, target migration and both endpoints of the same
forward-referencing row. They also verify following updates/deletes. The
bounded standalone recipe compiles that exact file, independently of link-cli:

```sh
bash experiments/issue-138/run-doublets-index-regression.sh 0.8.0
bash experiments/issue-138/run-doublets-index-regression.sh 0.9.0
```

All four assertions fail on 0.8.0 and all four pass on 0.9.0. Retained logs
record exit codes and child maximum RSS, at most 251,388 KiB across these
sequential probes. Cargo compilation, linker workers and test threads are
limited to one; the probe has a 1.5 GiB virtual-address limit and uses no
debug information. The initial 768 MiB cap allowed LLVM's default linker
workers to exhaust that allowance; its failure log is preserved separately.
It is not counted as regression evidence. A duplicate-pair test draft was
corrected to use distinct pairs before the final before/after runs.

The repository registry checker passes after the update, including Cargo
lockfile checks; npm reports zero vulnerabilities. The regular native CI
suite discovers the permanent tests. Complete suites and platform checks run
in CI, following the latest maintainer instruction; this focused probe does
not substitute for those checks or complete the wider acceptance inventory.
