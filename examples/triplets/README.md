# Triplets stored in doublets (issue #151)

`TripletsOverDoublets` keeps deep-foundation style triplets
`(index: type from to)` in any doublets store and gives them an associative
API: create, update in place, delete, `Each` / `Count` by index, type, from
and to, substitution queries with three parts, and Links Notation import and
export. The encoding and the cost of every operation are in
[docs/TRIPLETS.md](../../docs/TRIPLETS.md).

The Rust example imports a few people, moves `r` from `bob` to `carol` with
the substitution `((($r: knows alice bob)) (($r: knows alice carol)))`, prints
the triplets and the doublets behind them, and then stores 1011 triplets made
by the generator of the parity test to count their doublets:

```sh
cargo run --manifest-path rust/Cargo.toml --example triplets
```

It prints [expected-output.txt](expected-output.txt).

The same in C#:

```csharp
using Foundation.Data.Doublets.Cli;

using var named = new NamedTypesDecorator<uint>("db.links");
var triplets = new TripletsOverDoublets<uint>(named); // finds or creates Pair and Nothing
TripletsLino.Import(triplets, """
    (Type: Type 0 0)
    (Person: Type 0 0)
    (knows: Type 0 0)
    (alice: Person 0 0)
    (bob: Person 0 0)
    (carol: Person 0 0)
    (r: knows alice bob)
    """);
TripletsQueryProcessor.ProcessQuery(triplets, "((($r: knows alice bob)) (($r: knows alice carol)))");
var knows = triplets.GetByName("knows");
var carol = triplets.GetByName("carol");
var back = triplets.CreateTriplet(knows, carol, triplets.GetByName("bob"));
Console.WriteLine(triplets.CountTriplets(TripletQuery<uint>.ByType(knows))); // 2
triplets.DeleteTriplet(back);
Console.Write(TripletsLino.Export(triplets)); // ... (r: knows alice carol)
```

The tests of both languages are the same 18 cases:
`csharp/Foundation.Data.Doublets.Cli.Tests/TripletsOverDoubletsTests.cs` and
`rust/tests/triplets_over_doublets_tests.rs`.
