# Allocation errors through decorators

Rust's `NamedTypeLinks` interface retains its existing `create`, `ensure_created`
and `get_or_create` methods. Code that needs to handle allocation or journal
failures uses `try_create`, `try_ensure_created` and `try_get_or_create`. Each
returns `anyhow::Result<u32>` and composes through the transaction, version and
persistent transformation decorators.

```rust
use anyhow::Result;
use link_cli::NamedTypeLinks;

fn allocate(links: &mut impl NamedTypeLinks) -> Result<u32> {
    let point = links.try_ensure_created(8)?;
    let pair = links.try_get_or_create(point, point)?;
    links.save()?;
    Ok(pair)
}
```

The default fallible pair helper searches for an existing pair, then calls
`try_create` only if one is missing. Transaction and version adapters keep
their own admission checks. A custom backend that can fail allocation must
override its fallible methods; the default allocation methods preserve the
legacy infallible interface for existing implementers.

Persistent transformations fire after a successful allocation. Ensuring an
existing address or finding an existing pair does not fire a transformation.
The fallible methods also surface a trigger failure parked by an earlier
infallible call. Trigger schema, text and payload construction propagates
allocation errors rather than entering an infallible transaction bridge.

Named query planning also propagates failures from temporary address probes.
Successful probes create and then delete temporary rows in reverse order to
restore allocation order. They still execute the selected adapter's writes
and triggers. If allocation or deletion fails after earlier effects, those
effects may remain. Trigger bookkeeping can similarly leave reusable schema
or text rows before its final root is published.

A returned error does not prove that a write was undone or that its durable
outcome is known. Complete the selected transaction's rollback, save or recovery
protocol before another write, according to that adapter's ownership rules.
The decorator does not take over or dispose the caller's inner store. Existing
default-off tracing remains available on transaction, version and trigger
layers.

The pair helper pays for the selected backend's search and, when needed,
allocation. The trigger wrapper checks existing addresses or pairs to distinguish
no-ops; trigger execution and query probes retain their existing costs. This
change adds no background service, backend or dependency.

C# uses exceptions throughout this write path. Its regression controls verify
that the original allocation exception survives trigger/query composition and
that a following write persists after the fault is removed. The reproduced
infallible-bridge panic concerns Rust.

See the [case study](case-studies/issue-138/FALLIBLE-COMPOSITION-PLAN.md) for the
fault matrix and [transaction recovery](ENGINE-STRATEGIES.md) for owner cleanup.
