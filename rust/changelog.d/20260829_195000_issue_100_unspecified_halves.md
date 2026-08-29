---
bump: patch
---

Fixed unbound substitution variables and `*` in a substitution: they are now resolved at the write boundary the way C# resolves `Constants.Any`, so a created half becomes null, a lookup treats the half as a wildcard, and an update keeps the half already stored, instead of writing the literal `4294967295`.
