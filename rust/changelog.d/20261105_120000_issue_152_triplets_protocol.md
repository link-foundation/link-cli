---
bump: minor
---

Serve triplets over the Links Notation text and binary protocol (#152). `clink --serve --triplets` (`ServerOptions::triplets`) answers `(count: (i T f t))`, `(each: (i T f t))`, `(create: (T f t))`, `(update: (i T f t))`, `(delete: i)` and three-part substitution queries with `(i: T f t)` triplets, keeps names and `(auth: token)`, and leaves a server without the flag as it was. Subscriptions push triplet changes and permissions judge them through the triplets view of `PermissionsDecorator`. A binary client whose packet asks for `--arity 2..3` gets each triplet as one link of three references, `(i: (T f t))`, and every parser reads both forms. `RemoteLinks` implements `Triplets` and adds `subscribe_triplets` and `receive_triplet_notification`, `triplets_operations` holds the requests and replies, and `docs/protocol/triplets.txt` is the conversation both ports replay.
