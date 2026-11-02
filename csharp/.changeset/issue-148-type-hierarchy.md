---
'Foundation.Data.Doublets.Cli': minor
---

Add `TypesDecorator<TLinkAddress>` and `ITypes<TLinkAddress>` (#148): a type hierarchy `Type -> SubType -> … -> Value` with multiple inheritance kept in plain links. The type of a link is its source, the root is `(Type: Type Type)`, a sub-type is `(S: T S)` and extra parents are pairs `(p: S T2)` marked by `(m: Extends p)`. `TypeOf`, `IsType`, `ParentsOf`, `AncestorsOf`, `IsA` and `IsOf` terminate on cycles, and `Define`, `Extend` and `Instance` write. The decorator is not sealed, and pinned types are sub-types of its root.

Update Platform.Data.Doublets to 0.18.6 and System.CommandLine to 2.0.14. Counting `(* point)` now visits a self-referencing point once. Since 0.18.6 a merging update also ends by reporting the link it merged into, unchanged; `LinksUniquenessAndUsagesRepointingResolver` does not pass that notice on, so `clink --changes` and the protocol server still report only the links a write changed, as the Rust port does (creating a duplicate replies with no change).
