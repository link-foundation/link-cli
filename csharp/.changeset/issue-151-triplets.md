---
'Foundation.Data.Doublets.Cli': minor
---

Add `TripletsOverDoublets<TLinkAddress>` and `ITriplets<TLinkAddress>` (#151): triplets `(index: type from to)` stored in any doublets store with an associative API. A node `(i: T 0 0)` is the doublet `(i: T i)` and a link `(i: T f t)` is `(i: T ((Pair f) t))`, with the points `Pair` and `Nothing` found or created by name, so the type stays the source and `TypesDecorator` reads triplets as values of their types. `CreateTriplet`, `UpdateTriplet` (the address stays), `DeleteTriplet` (cascading) and `EachTriplet` / `CountTriplets` by index, type, from and to leave no unused wrapper or pair behind. `TripletsQueryProcessor.ProcessQuery` runs substitution queries with three parts, such as `((($r: knows alice bob)) (($r: knows alice carol)))`, and `TripletsLino` imports and exports `(index: type from to)` lines. The encoding and its costs are in `docs/TRIPLETS.md`.
