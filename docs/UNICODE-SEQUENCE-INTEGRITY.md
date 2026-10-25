# Unicode sequence graph integrity

Unicode strings in ordinary named storage use typed symbol leaves and binary
sequence links. A sequence is a graph: unchanged subtrees can appear more than
once. Each occurrence contributes its symbols, in source-before-target order.

Rust and C# now detect a sequence node appearing twice on the active traversal
path. Self-cycles and cycles through either child fail with a decoding error.
Missing children also fail; they cannot silently drop part of a Rust string.
Repeated valid subtrees remain valid. Traversal uses an explicit stack, so a
deep finite sequence does not consume one call-stack frame per graph level.
Each call owns its path and stack; a failed decode leaves no traversal state
for the next read. Name decoding propagates these failures.

| Surface | Behavior |
| --- | --- |
| Rust `UnicodeStringStorage::get_string` | Returns `Err` for cyclic/missing sequence graphs and existing UTF-16 errors. |
| Rust `unicode_sequence_code_units` | Returns `Err` for cyclic/missing graphs; retains raw UTF-16 code units, including lone surrogates. |
| Rust `RightSequenceWalker::try_walk` | Checked `Result<Vec<u32>>`, preserving every leaf occurrence. |
| Rust `RightSequenceWalker::walk` | Existing Vec signature; valid graphs retain their output, malformed graphs panic with guidance to use `try_walk`. |
| C# `GetString` / `UnicodeSequenceToStringConverter.Convert` | Throw `InvalidDataException` for cycles or missing sequence nodes; retain existing UTF-16 string behavior. |

C# keeps `Platform.Data.Doublets.Sequences`' maintained Unicode converter and
supplies validation through its existing `ISequenceWalker<T>` interface. This
does not fork the converter or change the stored representation. Rust keeps
its existing typed matcher, converters and public compatibility wrapper.

Active-path metadata costs O(depth), in addition to the existing stack and
decoded output. The walker expands shared subtrees for each occurrence rather
than skipping previously visited nodes. This is graph-integrity validation,
not a decoded-output budget: a compact valid DAG can represent large output.
No new physical repair or power-loss guarantee is supplied. Existing converter
memoization remains unchanged; cached results do not traverse the graph again.
This check applies when decoding actually traverses stored links.

The native before tests use bounded child processes. Linux Rust children have
128 MiB address-space/two-second CPU limits and a ten-second parent deadline;
C# children cap the managed heap at 128 MiB on every platform, add Linux CPU
limits, disable dumps and have a ten-second parent deadline. Children are
reaped. Actual-WASM portable contracts execute the same graph cases through
the maintained wasm-bindgen test runner; the coverage driver caps the test
module's linear memory at 256 MiB. The universal JS API has no direct
typed sequence-graph decoder; its existing actual-WASM client conformance is
checked without adding an unrelated public API.

Tests cover both directions of self/two-node cycles, repeated rejection,
healthy following reads, malformed name propagation, Unicode order, repeated
shared subtrees, 2,048-level valid sequences, missing children, existing UTF-16
policies and repeated native file reopening. See the
[work plan](case-studies/issue-138/UNICODE-GRAPH-PLAN.md),
[before evidence](../dev/log/issues/138/pulls/139/unicode-graph-before.txt)
and `experiments/issue-138/validate-unicode-graph.sh`.
