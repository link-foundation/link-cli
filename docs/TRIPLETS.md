# Triplets stored in doublets

Issue [#151](https://github.com/link-foundation/link-cli/issues/151).

deep-foundation's links are triplets `(id, type, from, to)` where the type is a
link too; link-cli stores doublets `(id: source target)`. `TripletsOverDoublets`
keeps triplets in any doublets store, with no second storage engine, and gives
them an associative API.

| | C# | Rust |
|---|---|---|
| decorator | `TripletsOverDoublets<TLinkAddress>` (`LinksDecoratorBase`) | `link_cli::TripletsOverDoublets<L>` |
| interface | `ITriplets<TLinkAddress> : INamedTypesLinks<TLinkAddress>` | `link_cli::Triplets: NamedTypeLinks` |
| a triplet | `Triplet<TLinkAddress>(Index, Type, From, To)` | `Triplet { index, type_id, from, to }` |
| a restriction | `TripletQuery<TLinkAddress>` (`Any`, `ByIndex`, `ByType`, `ByFrom`, `ByTo`) | `TripletQuery` (`any`, `by_index`, `by_type`, `by_from`, `by_to`) |
| create | `CreateTriplet(type, from, to)`, `CreateTripletPoint()` | `create_triplet`, `create_triplet_point` |
| read | `GetTriplet`, `IsTriplet`, `EachTriplet(query)`, `CountTriplets(query)` | `get_triplet`, `is_triplet`, `each_triplet`, `count_triplets` |
| update | `UpdateTriplet(index, type, from, to)` returns the triplet before | `update_triplet` |
| delete | `DeleteTriplet(index)` returns every deleted triplet | `delete_triplet` |
| encoding | `Markers`, `RoleOf(id)`, `DoubletUsages(id)` | `triplet_markers`, `role_of`, `doublet_usages` |
| substitution | `TripletsQueryProcessor.ProcessQuery(triplets, query)` | `process_triplets_query` |
| Links Notation | `TripletsLino.Export` / `TripletsLino.Import` | `triplets_to_lino` / `import_triplets_lino` |

Like every layer here, the decorator wraps any store of the same interface
(`INamedTypesLinks` / `NamedTypeLinks`) and passes the doublet API through, so
it composes with `TypesDecorator` (#148), transactions, permissions and the
file-backed stores. Every member is virtual (C#) or a default trait method
(Rust), so a decorator over it can reuse or override any of it. The tests,
`csharp/Foundation.Data.Doublets.Cli.Tests/TripletsOverDoubletsTests.cs` and
`rust/tests/triplets_over_doublets_tests.rs`, are the same 18 cases in both
languages.

## The encoding

The type of a link is its source, as in the type hierarchy of
[#148](../README.md#type-hierarchy) and in link-foundation/ui's `typeSystem`. Two marker points
are added, found or created by name: `Pair` (`P`) and `Nothing` (`N`, which
stands for the empty part `0`).

| triplet | doublets |
|---|---|
| `(i: T 0 0)`, a node | `(i: T i)` |
| `(i: T f t)` | wrapper `w = (P f)`, pair `p = (w t)`, outer `(i: T p)` |
| the same `(T f t)` again | `(T p)` is taken, so a duplicate pair `d = (p i)` and the outer `(i: T d)` |

A zero `f` or `t` inside a wrapper or a pair is written as `N`, so
`(i: T f 0)` is `(i: T ((P f) N))` and `(i: T 0 t)` is `(i: T ((P N) t))`.

For example, after `TripletsLino.Import` of the people of the tests and
`((($r: knows alice bob)) (($r: knows alice carol)))`, the store holds
(`cargo run --manifest-path rust/Cargo.toml --example triplets`):

```text
(1: 1 1)  Pair         marker
(2: 2 2)  Nothing      marker
(3: 3 3)  (Type: Type 0 0)
(4: 3 4)  (Person: Type 0 0)
(5: 3 5)  (knows: Type 0 0)
(6: 4 6)  (alice: Person 0 0)
(7: 4 7)  (bob: Person 0 0)
(8: 4 8)  (carol: Person 0 0)
(9: 5 12) (r: knows alice carol)   outer, at the address r had before the update
(10: 1 6) wrapper (P alice)
(12: 10 8) pair (alice carol); the pair (alice bob) at 11 was collected
```

### Reading a doublet back

Every doublet is classified by its sources, without a scan
(`RoleOf` / `role_of`):

- `P` and `N` are the markers;
- `(w: P x)` is a wrapper;
- `(p: w x)` whose source is a wrapper is a pair;
- `(d: p i)` whose source is a pair is a duplicate pair;
- `(i: T y)` is a triplet when `y` is `i` (a node), a pair, or a duplicate
  pair `(p i)` that points back at `i`;
- anything else is a doublet of somebody else (`Foreign`), such as the
  `Extends` wiring of `TypesDecorator`, and is not a triplet.

`GetTriplet` therefore reads at most four doublets: the outer, its target, the
pair behind a duplicate pair, and the wrapper.

### Why this encoding

The issue names two candidates: *type as source* `(i: T (f t))` and *pair of
pairs* `(i: (T) (f t))` with marker links. The chosen encoding is the first
one with the from-part wrapped in `(P f)`. The three were measured on the same
workload by [`experiments/issue-151/encodings.py`](../experiments/issue-151/encodings.py):
1011 triplets created with the generator of the parity test, without its
updates and deletes (a root, 10 types, then 1000 triplets with a random type
and a from and a to that are empty one time in four; 81 of them are nodes), in a store with unique `(source, target)` pairs
and source and target indexes, as link-cli's stores are. "Visited" counts the
doublets an `Each` reads through the indexes, including the one it reads to
classify a candidate, averaged over every triplet as the queried part; "noise"
is visited minus results, and "worst" is the largest noise of one query.

| encoding | doublets | per triplet | Each by type: visited / noise / worst | by from | by to | wrong answers |
|---|---|---|---|---|---|---|
| **chosen**: `(i: T p)`, `p = ((P f) t)` | 2317 | 2.29 | 2.0 / 1.0 / 14 | 2.5 / 1.7 / 7 | 3.2 / 2.4 / 16 | 0 |
| bare type as source: `(i: T p)`, `p = (f t)` | 1881 | 1.86 | 3.4 / 2.4 / 18 | 2.5 / 1.7 / 15 | 2.3 / 1.6 / 14 | 2 |
| pair of pairs: `(i: (Ty T) p)`, `p = ((P f) t)` | 2829 | 2.80 | 3.0 / 2.0 / 15 | 2.5 / 1.7 / 7 | 4.2 / 3.5 / 18 | 0 |

The real decorator stores the same 1011 triplets in 2317 doublets besides the
two markers, as the example prints, which matches the simulation.

- **Bare type as source is ambiguous.** Its pair `(f t)` has the from-part as
  its source, which is exactly how a link of type `f` looks. When `f` is the
  type of a node `t`, the pair `(f t)` *is* the node `(t: f t)` in a store
  with unique pairs, and the triplet's parts can no longer be read: in the
  workload above this happens once and gives two wrong answers. Even without a
  collision, every pair of from-part `f` shows up among the doublets of type
  `f`, so `Each` by type reads them too.
- **Pair of pairs** answers correctly but takes one more doublet per used type
  and, more importantly, the type is no longer the source: the outer's source
  is `(Ty T)`, so the type hierarchy of #148 (`TypeOf`, `IsA`, `ParentsOf`,
  and every query that reads `(v: T x)` as a value of `T`) no longer works on
  triplets as they are. Each by type needs a lookup of the wrapper first.
- **The chosen encoding** keeps the type as the source, so a triplet node
  `(i: T 0 0)` is exactly a value `(i: T i)` of #148 and
  `new TypesDecorator(links)` reads triplets as values of their types (the
  test `Works_With_The_Type_Hierarchy`). The wrapper `(P f)` puts every pair
  under the marker `P`, so pairs never look like triplets of a type, and costs
  one doublet per distinct from-part.

## Costs

`k` is the number of triplets returned, and "index" means the source or
target index of the store (`query_links` / `Each` with one part set). All
counts are of doublets read; none of the reads below scans the store except
where said.

| operation | cost |
|---|---|
| `GetTriplet`, `IsTriplet`, `RoleOf` | at most 4 reads |
| `CreateTriplet(T, 0, 0)` | 1 create |
| `CreateTriplet(T, f, t)` | 2 searches, then up to 3 creates (wrapper, pair, outer); a duplicate adds a search and the duplicate pair |
| `UpdateTriplet` | the reads of a create, one update of the outer in place (the address stays), then the collection of the old pair |
| `EachTriplet` by index | `GetTriplet` |
| by type `T` | the doublets whose source is `T` (one index read), each classified: these are the triplets of type `T` and any foreign doublets with source `T` (wrappers start at `P`, pairs at a wrapper, duplicate pairs at a pair, so none of them is read) |
| by from `f` (with or without type / to) | search `(P f)`, the pairs whose source is that wrapper, and for each pair the outers that end at it and the duplicate pairs that start at it |
| by from `f` and to `t` | search `(P f)` and `(w t)`: two searches, then the triplets on that pair |
| by to `t` | the doublets that end at `t` (pairs `(w t)`, the node `t` itself, duplicate pairs `(p t)`), each classified; then the triplets on each pair |
| no restriction, or only an empty from or to | every doublet is read and classified, since `(P N)` pairs alone miss the nodes |
| `CountTriplets` | `EachTriplet` |
| `DeleteTriplet(i)` | the users of `i` (Each by type, from and to of `i`), recursively; the auxiliary doublets that touch them; the deletes; then collection |

### Deletion and shared pairs

Wrappers and pairs are shared: every triplet with the same from-part uses one
wrapper, and every triplet with the same from and to uses one pair. They are
not reference-counted, since a count would be one more doublet to keep in
step. Instead, a wrapper or pair is collected when the store's indexes say
nothing uses it any more: `DoubletUsages(id)` is the number of other doublets
whose source or target is `id`, which is two index reads. A collected pair
leads on to its wrapper (and a duplicate pair to its pair), so an update or a
delete collects at most three doublets per triplet, each after two index
reads.

A delete removes, like a doublet delete in link-cli, every triplet that uses
the deleted one as its type, from or to, recursively, and returns them all,
the deleted one first. Users are deleted first. Before any delete, the wrappers
and pairs that touch the deleted triplets are noted, because a store that
deletes what uses a deleted doublet (the `DecorateWithAutomaticUniquenessAndUsagesRepointing`
stack in C#) may take some of them along; their sources are then collected too.
The tests check after every operation that no wrapper or pair is left unused
(`AssertNoOrphans`), and that deleting every triplet leaves only the two
markers.

### Address stability

A triplet's address is its outer doublet's, and an update rewrites the outer
in place, so references to the triplet stay valid when its type, from or to
change. If the new `(T p)` is taken by another triplet, the update goes
through a duplicate pair; if it would turn the outer into a doublet that
already exists (which the encoding never produces for triplets, but a foreign
doublet could), the update is refused rather than merging two links.

## Substitution queries

`TripletsQueryProcessor.ProcessQuery` / `process_triplets_query` take the
query of the issue, `((restriction) (substitution))`, with three parts per
pattern:

```text
((($r: knows alice bob)) (($r: knows alice carol)))   update the to-part of r in place
((($i: knows $f $t)) (($i: likes $f $t)))             change the type part
((($i: likes $f $t)) (($i: * $t $f)))                 * keeps a part; swap from and to
((($p: Person 0 0)) (($p: Person 0 0) (knows $p bob)))  create one triplet per match
(() ((likes: Type 0 0) (likes carol alice)))          no restriction: create; a new name names a new triplet
((($i: knows $p bob)) ())                             no substitution: delete
((($i: likes $f $t) ($f: Person 0 0)) (...))           a conjunction; variables join patterns
```

A reference is a variable `$x`, `*` (anything; in a substitution, "keep"), an
address, `0` for an empty from or to, or a name. Patterns of the restriction
are solved by backtracking: each pattern becomes one `EachTriplet` with every
part already known (an address, a name, or a variable bound by an earlier
pattern), which takes the cheapest path of the table above. Restriction patterns missing from the substitution are deleted,
substitution patterns missing from the restriction are created, and those in
both are updated in place. A query that refers to an unknown name, an unbound
variable, a `*` where a value is needed, or a part that is not a triplet is
refused before anything is written; a restriction by an unknown name matches
nothing.

## Links Notation

`TripletsLino.Export` / `triplets_to_lino` writes one `(index: type from to)`
per triplet in the order of the addresses, a part by its name when it has
one, by its address otherwise, and `0` when empty:

```text
(Type: Type 0 0)
(Person: Type 0 0)
(knows: Type 0 0)
(alice: Person 0 0)
(bob: Person 0 0)
(carol: Person 0 0)
(r: knows alice bob)
```

`TripletsLino.Import` / `import_triplets_lino` reads the same lines back: a
numeric index is kept, a named index is the triplet of that name (created
when there is none), and a line without an index is a new triplet. Lines may
refer to each other in any order. Variables, `*`, markers and doublets that
are not triplets are refused. Export, import into a new store and export again
gives the same text, names and addresses included (the test
`Lino_Round_Trip_Keeps_Names_And_Addresses`).

## Protocol

`clink --serve --triplets` (`ServerOptions.triplets` in Rust,
`LinksServerOptions.Triplets` in C#) serves the triplets of the store over the
text and the binary protocol. `RemoteLinks` in both ports is a `Triplets` store
(`ITriplets` in C#) behind such a server, and the `*Triplet*` calls of
`@link-foundation/link-cli-client` are its JavaScript client.

| Request | Reply |
|---|---|
| `(count: (i T f t))` | `(count: N)` |
| `(each: (i T f t))` | one `(i: T f t)` per match, by address |
| `(create: (T f t))` | `() ((i: T f t))` |
| `(create: (0 0 0))` | `() ((i: i 0 0))`, a point |
| `(update: (i T f t))` | `((i: T0 f0 t0)) ((i: T f t))` |
| `(delete: i)` | `((i: T f t)) ()` per removed triplet, `i` first |
| `(subscribe: (i T f t))` | `(subscription: N)`, then `(notify: N)` pushes |
| a substitution query, `() ((T f t))` | its changes, naming what has a name |
| an empty document (`clink --connect` with no query) | every triplet |

A restriction holds zero to four parts, matched part by part: those left out and
`*` match anything, and `0` is an empty from or to. Names, `(unsubscribe: N)`
and `(auth: token)` are the same as without triplets. The wrappers and pairs
never appear, so the addresses of triplets are not consecutive. The recorded
conversation is [protocol/triplets.txt](protocol/triplets.txt); both ports
replay it in text and in binary with every option, and the JavaScript client
replays it in text against both servers. The doublet conversation
([protocol/links-operations.txt](protocol/links-operations.txt)) is unchanged.

**A mode, not a request form.** `(delete: i)`, `(count: ())`, restrictions of up
to three parts and substitution queries read the same for doublets and for
triplets, so their shape cannot say which one is meant, and a per-request form
would change those requests for every client. A server without `--triplets`
answers exactly as before and refuses `(create: (T f t))`. In triplet mode every
request is read as triplets, so `(create: (1 2))` is refused, and the doublet
calls of `RemoteLinks` are refused or fail like a lost connection, except
`exists` and the names.

**Subscriptions.** `(subscribe: ...)` takes a triplet restriction and pushes
triplet changes. A side that does not fit is empty, so a triplet that stops being
of the type asked for is gone for the subscriber and one that becomes it is new.

**Permissions.** `--permissions` works in this mode: the permission layer sits
above `TripletsOverDoublets` (C# `TripletsPermissionsDecorator`, Rust
`triplets/permissions.rs`) and judges the outer doublet of every triplet, the
one that holds its type. Writing a triplet also writes its wrappers and pairs
inside the same transaction, below the check. Creating needs the right to create
and to write the type, not to read it; a substitution query does read it,
because it has to match. Every part needs the right to refer to it (#161): a type
it may write is enough, a part it may not refer to answers as a missing one, and
a part the triplet already holds may stay. Deleting needs the right to write every
triplet the delete takes along, or nothing is deleted.

**`auto_create_missing_references`** does not apply: a triplet query creates no
reference it cannot find.

**Binary replies.** Under the default arity, a triplet travels as doublets, like
any other link. `(i: T f t)` is a link with an id, and in the binary mapping that
is `(Identified i T f t)`, five references, so a reply under `--arity 2..3`
would get no benefit from the arity. A reply or push whose arity holds three
but not five therefore carries `(i: (T f t))`, whose `(T f t)` is **one link of
three references**; see
[protocol/binary-links-notation.md](protocol/binary-links-notation.md#73-triplets).

## Limits

- Restricting only by an empty from or to (`ByFrom(0)`, `ByTo(0)`) reads
  every doublet, since nodes have no pair; with a type it reads the triplets
  of that type instead.
- `TripletsOverDoublets` adds no index of its own: the costs above assume the
  store indexes sources and targets, as the united and split memory stores of
  C# and the indexed Rust stores (`with_indexed_lookups`) do.
- GraphQL over triplets (#153) is a separate issue.
- A union or a remap does not translate triplets over the protocol: the
  JavaScript `UnionLinks` and `RemapLinks` refuse those calls, so send them to a
  member or to the store below.
