# Standard queries over a private causal workspace

The standard Rust and C# protocols and the selected actual-WASM/JS session transport
accept one top-level link named `query`, with exactly one reference containing
the original query text. Construct this reference with the notation formatter;
quotes, newlines and literal-reference envelopes inside the query remain text.

```text
(query: '() ((1: 1 1))')
```

The same canonical document can travel through the existing text or binary
framing. The receiver parses the inner text with the ordinary query parser,
preserving quoted/encoded names that resemble numeric addresses, `*` or `$x`.
Malformed query envelopes reject. Empty inner text exports the named rows.
Clients must select a peer that supports this envelope; a failed request is
never resent as a different representation after an unknown outcome.

Existing operation requests and direct query documents remain available.
`count`, `each`, create/update/delete and name requests use the selected address
type; operation replies carry decimal numeric references. A restriction's `*`
is a separate optional part, so the maximum u64 identity remains addressable.
Query/export replies substitute stored names and retain the established
before/after change shape. Their canonical string model does not retain the
name/address distinction for subsequent query replay.

The checked workspace uses a wider internal address type, admits stored
addresses from zero through u64::MAX, and retains a separate wildcard marker.
Zero is a concrete source/target reference and remains bound during variable
matching, repeated-variable equality, joins and substitution. A missing
variable-map entry means unbound. Row identities remain nonzero; new row
identities must lie in the caller's disjoint reservation, while known retained
identities may be recreated. All changes stay private until explicit-token
session publication. Query saves do not publish.

The shared processor previews the query against owned rows/names and reserves
its exact allocations before applying real edits. Checked input, matching and
reply admission rejects unsupported requests before those edits. See
[the session contract](../ENGINE-CAUSAL-SESSIONS.md) for exact limits, owned
copies, preparation, retry behavior and costs. These bounds apply to checked
sessions; the existing ordinary u32 facade retains its selected store policy.

[`causal-query-v1.json`](causal-query-v1.json) specifies independent finite
behavioral vectors. Every case supplies an inclusive fresh reservation,
initial retained rows, original query texts with expected success/error,
expected final private rows, the next allocated identity and the final edit
count after creating/deleting that next identity. Addresses use canonical
decimal strings; names are nullable strings. Cases are independent, so their
publication tokens and reservations need not be globally unique.

Native Rust, C# checked sessions, linked actual WASM and actual Node/Bun/Deno
consume these vectors. C# also round-trips each case through text and all binary
codec options before standard dispatch. The JS consumers assert that staging
and preparation preserve real provider bytes and publication count. The
physical recipes now use those operations and original-text queries with exact
independent versions and retained retry requests. Focused C# physical checks
pass; [the physical parity plan](../case-studies/issue-138/CAUSAL-PHYSICAL-QUERY-PLAN.md)
tracks complete five-runtime CI verification.
