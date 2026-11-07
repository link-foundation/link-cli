# Additional competitor source audit

Requirement mapping: R5/R11/R12.

- [x] Retrieve current primary documentation and pin MySQL, MongoDB, Cassandra
  and Accord source commits; read relevant functions, not just search snippets.
- [x] Compare durable high-watermarks, acknowledgement deadlines, ordinary
  quorum writes/repair and current leaderless transaction routing separately.
- [x] Extend source observations and configuration comparison with concrete
  implementation implications and explicit unsupported guarantees.
- [ ] Implement and verify the remaining useful strategies across all surfaces;
  the research record does not satisfy this implementation requirement.

Source downloads, exact commit timestamps and inspection notes are preserved
under experiments/issue-138. No source implementation was copied. See RESEARCH.md
for permanent primary links and line ranges. Current Cassandra includes Accord
routing; older descriptions of ordinary Dynamo writes do not describe every
current transaction path.
