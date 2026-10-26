# Deep structure output

Rust's concrete `LinkStorage::format_structure`, the `NamedTypeLinks` default
formatter and C#'s `LinoDatabaseOutput.FormatStructure` expand a link's source
spine. The target remains a reference even when it is also the source. Names
retain the existing quoting rules. A source already on the active path becomes
a reference, so cycles remain finite output rather than decoding errors.

These entry points now use explicit frames. A finite 2,048-node chain no longer
requires 2,048 native call-stack frames. The formatter resolves the terminal
source, then target/index references from deepest node to root, preserving the
existing order of decorator calls and name-resolution errors. It assembles the
prefixes, terminal reference and suffixes into one growable output buffer,
avoiding a new whole-subtree string at each ancestor.

Traversal metadata costs O(depth), plus resolved reference text and the output.
The assembly work is linear in output bytes; store lookup and name-decoding
costs still depend on the chosen implementation. This adds no output budget,
cache, stored representation, repair or new durability guarantee.

Rust retains the public `format_structure_recursive` method name and signature.
`format_structure` still dispatches through that method, so an implementation's
entry-point override remains available. Its default implementation now expands
the source spine itself with explicit frames. Caller-supplied visited entries
are retained; successful traversal removes only entries it added. Errors retain
the same path progress and reference-read order as the previous default.

Bounded automated before tests reproduce stack overflow in both Rust entry
points and C#. Each native child has finite memory/CPU limits and a parent
deadline; worker stacks are 128 KiB for Rust and 256 KiB for C#. Independent
shallow oracles verify names, cycles, shared children and failure ordering.
Portable Rust contracts also execute in actual WASM. Native persistence checks
reopen the ordinary Unicode named store twice.

The separate [legacy Rust text-name format](TEXT-STORAGE.md) has its own
lossless-name regression tests and versioned metadata. That persistence change
is tracked independently from traversal. This document does not claim all persistence or coverage requirements
are complete. See the [execution plan](case-studies/issue-138/STRUCTURE-DEPTH-PLAN.md),
[before evidence](../dev/log/issues/138/pulls/139/structure-depth-before.txt) and
`experiments/issue-138/validate-structure-depth.sh`.
