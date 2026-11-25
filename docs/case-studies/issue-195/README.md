# Issue 195 Case Study: Transactional Writes That Slow Down With History

Issue: <https://github.com/link-foundation/link-cli/issues/195>

Pull request: [#198](https://github.com/link-foundation/link-cli/pull/198)

This folder holds the issue data, the restated requirements, the root cause, the design and the measurements for this fix.

## 1. Issue summary

With `--transactions`, each `clink` call took longer than the one before, even when the live store stayed small. The transitions store (`<db>.transitions.links` and `<db>.transitions.names.links`) grew by about 4.7 KB per write. `--retention sized:64` did not bound it. In the issue's run, write 40 took over 4 s, while the same loop without `--transactions` stayed at 12–23 ms.

hive-mind works around this by renaming the transitions files aside once they pass 64 KiB (link-assistant/hive-mind#4143).

## 2. Requirements

| ID | Requirement | Source |
|----|-------------|--------|
| R1 | The cost of a write does not grow with the number of prior writes. | "Expected" |
| R2 | A retention policy bounds the journal on disk. | "Expected", suggested fix 2 |
| R3 | Transitions are stored in an append-only log, such as `FileTransitionLog`, so each call appends its own transitions instead of loading and rewriting the history. | Suggested fix 1 |
| R4 | CI runs a benchmark that fails when write N+100 is much slower than write N on a constant-size store. | Suggested fix 3 |
| R5 | Rust, C# and JavaScript change together, with parity and tests (AGENTS.md). | Repository rules |
| R6 | Existing databases keep their history: a journal written by an older `clink` still works. | Implied by R3, because the on-disk format changes |

## 3. Root cause

`clink --transactions` opened the journal as a full links store: a `NamedTypesDecorator` over `<db>.transitions.links`. Each journal entry was the name of a new link, so it landed in `<db>.transitions.names.links`. Opening the store parsed every link and every name. Closing it wrote every link and name to `<path>.saving` and renamed that file over the original. Both steps cost O(history), and every invocation ran both. So write *n* cost O(*n*), and *n* writes cost O(*n²*) in total.

Retention made it worse: `LogRetentionPolicy` trimmed only the in-memory record list after a commit. Nothing removed entries from the store on disk, so the file grew under every policy, and so did the cost of loading it at startup.

The same design existed in all three implementations: Rust `rust/src/main.rs`, C# `Program.cs`, and JavaScript `main-transactions.js`.

## 4. Design

### 4.1 Append-only journal file (R3, R1)

The CLI journal is now a text file, `<db>.transitions.log` (or the `--transactions-file` path), with one entry per line. Each language has the same `FileTransitionLog`:

| | Rust | C# | JavaScript |
|-|------|----|------------|
| Log | `rust/src/transactions/log.rs` | `FileTransitionLog.cs` | `transactions-log.js` |
| CLI journal and legacy import | `rust/src/transactions/journal_file.rs` | `TransitionsJournal.cs` | `transactions-journal-file.js` |
| Compaction | `rust/src/transactions/compaction.rs` | `TransactionsDecorator.Compaction.cs` | `transactions-compaction.js` |

A write appends its transitions and its commit, rollback and applied markers, then fsyncs. Opening the log reads it once. A torn last line, left by a crash in the middle of an append, is truncated away. A line break inside an entry is rejected, so one entry is always one line.

### 4.2 Retention compacts the journal (R2)

After each commit or rollback, the decorator counts the entries it has appended. Compaction runs when all of these hold:

- the count exceeds `2 × (3 × retained records + 3) + 256`;
- the policy is not `Infinite`;
- the log supports compaction (a plain file does; a links store does not);
- no transaction is open and undecided;
- no version replay or recovery is pending.

It then rewrites the journal to the entries that still matter:

1. a `__transactions:sequence:<n>` watermark, so sequence numbers that version-control tags and branches refer to are never reused;
2. an applied marker for the highest applied sequence;
3. every retained record, unchanged;
4. the decision of each retained record's owner, plus the newest decision overall, which publication recovery reads;
5. the applied markers of the retained records.

The rewrite goes to `<path>.compact`, which is fsynced and renamed over the log, and then the directory is synced. A crash leaves either the old journal or the new one. A leftover `.compact` file is deleted on the next open. Because a rewrite happens only after the journal has doubled, its cost is spread over many appends, so each write costs O(1) amortized.

### 4.3 Migration (R6)

On open, an existing `<db>.transitions.links` store is copied into a temporary file, entry by entry. That file is fsynced and renamed to `<db>.transitions.log`, and only then are the legacy files deleted. If a crash leaves both, the legacy store is deleted only when the journal already holds every one of its entries. An explicit `--transactions-file` that still contains a links store is converted in place the same way. The journal file is recognized because every entry starts with `__transactions:`.

### 4.4 Library API

The decorators still accept a links store as their log (`NamedTypesDecorator` in Rust and C#, and the equivalent in JS), so existing library users keep working. That kind of log keeps every entry, because it does not support compaction. The READMEs now recommend `FileTransitionLog`. `MakeTransitionsDatabaseFilename` and its equivalents still return `.transitions.links`, for library users who rely on that path. The CLI opens its journal through `open_transitions_journal` (Rust), `TransitionsJournal.Open` (C#) or `openTransitionsJournal` (JavaScript), which name it with `make_transitions_log_filename` / `MakeTransitionsLogFilename`.

### 4.5 CI benchmark (R4)

`.github/scripts/transactions-benchmark.mjs` runs `clink --transactions --retention sized:64` 125 times on a two-link store. Each write swaps the target of link 1 between 1 and 2, so the live store never grows and only the history does. It compares the median of writes 11..25 with the median of writes 111..125. It fails when the later median exceeds 2 × the earlier one + 50 ms. The 50 ms of slack absorbs process start-up noise on shared runners.

It runs in `rust.yml`, `csharp.yml` and `js-server.yml` against the binary each workflow builds. Its own unit tests (`transactions-benchmark.test.mjs`) run in `npm run test:js`.

## 5. Measurements

The host is a shared 6-CPU VM with a load average of 4–9, so the absolute numbers are noisy. The trend is what matters.

### 5.1 The issue's reproduction (`experiments/issue-195/perf.sh`)

This is the same loop as in the issue: `--retention sized:64`, and a new hive-mind state written each time. The size columns are `s.links`, `s.names.links`, and then either the transitions links and names files (main) or the transitions log (branch).

| Write | Rust, main | Rust, this branch | JavaScript, main | JavaScript, this branch |
|------:|-----------:|------------------:|-----------------:|------------------------:|
| 3 | 575 ms | 265 ms | 898 ms | 504 ms |
| 20 | 1428 ms | 122 ms | 876 ms | 479 ms |
| 40 | 3969 ms | 145 ms | 982 ms | 442 ms |
| 60 | 8370 ms | 140 ms | n/a | n/a |
| Journal after the last write | 13,743 + 303,435 B, still growing | 32,568 B, cycles between 16 and 36 KB | 9,192 + 207,530 B, still growing | 35,089 B, cycles between 16 and 36 KB |

Logs: `evidence/reproduction-rust-main.log`, `evidence/reproduction-rust-branch.log`, `evidence/reproduction-js.log`.

The C# `--in` importer rejects the issue's input both on main and on this branch: `Invalid LiNo import line 1: Expected ')'`. It fails on any nested reference such as `(a: a (b c))`, which Rust accepts. That is a separate, pre-existing parity gap, not part of #195. For C#, the benchmark below, which uses plain queries, gives the measurement.

### 5.2 The CI benchmark (`transactions-benchmark.mjs`)

| Build | Writes 11..25 | Writes 111..125 | Limit | Result | Journal after 125 writes |
|-------|--------------:|----------------:|------:|--------|--------------------------|
| Rust, main | 144.8 ms | 566.7 ms | 339.6 ms | fails | 3,732 + 72,948 B |
| Rust, this branch | 40.2 ms | 47.5 ms | 130.3 ms | passes | 22,312 B |
| C#, main | 866.1 ms | 1011.9 ms | 1782.1 ms | passes | 12,320 + 250,336 B |
| C#, this branch | 813.9 ms | 792.9 ms | 1677.8 ms | passes | 22,312 B |
| JavaScript, main | 501.9 ms | 601.5 ms | 1053.9 ms | passes | 3,732 + 73,128 B |
| JavaScript, this branch | 374.1 ms | 544.7 ms | 798.1 ms | passes | 22,312 B |

On this branch, the journal is the same 22,312 bytes in all three languages after 125 writes. On a 400-write run of the branch's JavaScript build, the median per 25-write bucket stays flat between 303 and 390 ms (`evidence/write-curve-js-branch.log`, produced by `experiments/issue-195/write-curve.mjs`).

**Limitation of the CI check.** On main, the C# and JavaScript journals still grow without bound. But their startup cost (about 650–850 ms for .NET, 350–500 ms for Node) is large compared with the extra parse time of a store this small: about 1 ms per write. So 100 extra writes stay under 2 × + 50 ms. The check therefore catches the Rust regression on main reliably, and it guards all three builds against regressions of that size. It would not catch the slow C#/JavaScript growth within 125 writes. Tightening the ratio would make it flaky on shared runners. The tests in §6 that bound the journal size catch the on-disk growth in every language.

## 6. Tests

| Language | File | What it covers |
|----------|------|----------------|
| Rust | `rust/tests/transactions_journal_compaction_tests.rs` (12 tests) | sized/chunked retention bounds the file across reopen; infinite retention keeps everything; rolled-back owners and the newest decision survive; sequence watermark (reservation, positive integers only); atomic replace and stale `.compact`; filename helpers; links-store detection; legacy import once, in-place conversion, crash with both present; `clink` end to end |
| C# | `csharp/Foundation.Data.Doublets.Cli.Tests/TransactionsJournalCompactionTests.cs` (25 cases) | the same, plus torn-tail truncation (including across read chunks), undecided owners, default interface members, and the facade disposer releasing a file journal |
| JavaScript | `js/server/test/transactions-compaction.test.mjs` (21 tests) | the same contract as Rust |
| CI script | `.github/scripts/transactions-benchmark.test.mjs` | argument parsing, the constant-size query sequence, median/limit evaluation, window selection, and `main` passing a steady fake `clink` and failing one that slows down |

## 7. Files in this folder

- `github-data/`: the issue, its comments (none), and the PR as fetched when this was written.
- `evidence/`: the raw logs for the tables above.
