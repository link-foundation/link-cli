---
'Foundation.Data.Doublets.Cli': patch
---

Preserve quoted query references as names, including numeric names such as "0"
and "1", throughout reference validation, creation, matching and mutation. Bare
numbers remain link addresses. Encoded reference literals also resolve as names.
Stored triggers preserve this distinction when normalized and replayed, and
numeric names exported as LiNo can be replayed through queries.
