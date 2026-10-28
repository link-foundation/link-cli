# Author-fence admission before parsing

R1/R6/R7/R8: the wire contract promises a 256-byte record budget before JSON
parsing. Accepted fields are ASCII. JS/C# currently count UTF-16 units before
parsing, so a short non-ASCII invalid record can exceed that byte budget and
still reach the JSON parser. Its eventual refusal does not verify early admission.

- [x] Add focused tests that distinguish admission from JSON parser rejection;
      preserve the original failures without any provider publication.
- [x] Reject non-ASCII records before parsing in all ports, preserving every
      canonical accepted record and the existing byte/entry budgets.
- [x] Regenerate independent shared rejection vectors; run focused native and
      C# contracts plus strict Node coverage and Bun/Deno parity.
- [ ] Retain evidence, commit the correction, publish only the prepared branch
      and inspect complete exact-head CI without claiming wider completion.
