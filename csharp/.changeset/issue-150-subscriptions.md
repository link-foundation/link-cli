---
'Foundation.Data.Doublets.Cli': minor
---

Added subscriptions to the LiNo protocol server (#150). `(subscribe: (index source target))` answers
`(subscription: N)`, and every later change another connection makes to a link matching the pattern is pushed to the
subscriber as `(notify: N)` followed by its `(before) (after)` changes; `(unsubscribe: N)` answers nothing, and closing
the connection drops its subscriptions. `LinksServerOptions.Readable` hides links from subscribers (a link that becomes
unreadable arrives as a deletion) and `LinksServerOptions.MaxPendingNotifications` disconnects a subscriber that stops
reading. `LinksClient` sets pushes aside for `ReceiveNotification`, and `RemoteLinks` gains `Subscribe`, `Unsubscribe`
and `ReceiveNotification`.

Updated Platform.Data.Doublets to 0.18.7.
