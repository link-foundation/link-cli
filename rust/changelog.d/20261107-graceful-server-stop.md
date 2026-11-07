---
bump: minor
---

### Added

- `clink --serve`, with or without `--graphql`, now stops cleanly on Ctrl-C and on termination signals: it finishes the current request, saves the store and exits with status 0. `GraphqlServer::shutdown_handle` stops a GraphQL server from another thread, as `LinksServer::shutdown_handle` already did.
