---
'Foundation.Data.Doublets.Cli': minor
---

Serve triplets over the Links Notation text and binary protocol (#152). `clink --serve --triplets` (`LinksServerOptions.Triplets`) answers `(count: (i T f t))`, `(each: (i T f t))`, `(create: (T f t))`, `(update: (i T f t))`, `(delete: i)` and three-part substitution queries with `(i: T f t)` triplets, keeps names and `(auth: token)`, and leaves a server without the flag as it was. Subscriptions push triplet changes and permissions judge them through the new `TripletsPermissionsDecorator`. A binary client whose packet asks for `--arity 2..3` gets each triplet as one link of three references, `(i: (T f t))`, and every parser reads both forms. `RemoteLinks` implements `ITriplets<uint>` and adds `SubscribeTriplets` and `ReceiveTripletNotification`, `TripletsOperation` holds the requests and replies, and `docs/protocol/triplets.txt` is the conversation both ports replay.
