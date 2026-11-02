---
bump: minor
---

Add the `Types` trait and `TypesDecorator` (#148): a type hierarchy `Type -> SubType -> … -> Value` with multiple inheritance kept in plain links, the port of the C# `TypesDecorator` with the same tests. The type of a link is its source, the root is `(Type: Type Type)`, a sub-type is `(S: T S)` and extra parents are pairs `(p: S T2)` marked by `(m: Extends p)`. `type_of`, `is_type`, `parents_of`, `ancestors_of`, `is_a` and `is_of` terminate on cycles, and `define`, `extend` and `instance` write. `NamedTypeLinks` gains `query_links`, which finds links by a pattern.
