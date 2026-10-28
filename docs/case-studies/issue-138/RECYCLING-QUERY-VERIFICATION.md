# Exact legacy query allocation previews

The legacy reference planner counted anonymous composites, then created and
deleted that many probe rows on the real allocator. This did not include
preceding explicit/named allocations or duplicate merges in the same query.
On an empty store, `() ((1: 1 1) (2 2))` and
`() ((foo: foo foo) (2 2))` were rejected even though both create points 1/2.
`() ((100: 100 100) (99 99))` was also rejected, while
`() ((100: 100 100) (1 1))` incorrectly succeeded with a dangling reference.

On `cbddc1343bbc07a1d5b8b52d2772c7749c18fb89`, four of the first five
Rust regressions fail before production changes. All five independently
written C# cases also fail on that same head. The retained `*-before.log`
files record the actual validation errors and unexpected successful writes.

Rust now runs the existing processor on an owned in-memory copy of the exact
rows, names, allocation watermark and free-list order. A named decorator
copies both its data and Unicode names sidecar. Selected indexes are rebuilt
on the copy. Preview disables reference-validation recursion alone; the
normal matching, mutation, uniqueness, merge and cascade paths still run.
It performs no provider I/O or journal writes. Transaction/version forwarding
first checks the existing write fences; actual query execution retains its
normal observed journal and publication behavior.

Nine focused native regressions pass, including real file-byte preservation,
and the eight portable cases are linked into the actual-WASM suite. Each bare
and named-store case runs with and without selected lookup indexes. Existing
processor/change-report/parity targets pass 55 additional tests before the
final filesystem-only addition. Strict focused Clippy and Rust formatting
pass. The reproducible native target list is:

```sh
bash experiments/issue-138/run-recycling-query-tests.sh
```

This preview allocates O(data + names + free-list) storage and executes the
query once before real execution; selected index rebuilding has its usual
index costs. Auto-created references may require repeated previews. Bare
storage operations do not select a preview. Custom/remote stores and trigger
decorators without an exact preview implementation retain their compatibility
fallback and need separate work; this fix does not assert their parity.

C# heap-image preview and lifetime/transaction/history controls are under
focused verification. Complete suites, linked WASM execution and platform
coverage belong in CI under the latest maintainer instruction. Neither this
fix nor passing CI completes the wider R1–R12 acceptance inventory.
