---
'Foundation.Data.Doublets.Cli': patch
---

An update into an existing doublet merges into it and now re-points every link that used the merged-away address at the surviving one, as the Rust port does. Before, the `MergeUsages` of Platform.Data.Doublets 0.18.1 ([Data.Doublets#515](https://github.com/linksplatform/Data.Doublets/issues/515)) blanked the half of the usage instead, so `() ((1 2) (2 1))` followed by `((1: 1 2)) ((1: 2 1))` left `(2: 2 0)` rather than `(2: 2 2)`. Stores are composed with the new `DecorateWithAutomaticUniquenessAndUsagesRepointing()`, whose top layer, `LinksUniquenessAndUsagesRepointingResolver`, replaces `LinksCascadeUniquenessAndUsagesResolver`.
