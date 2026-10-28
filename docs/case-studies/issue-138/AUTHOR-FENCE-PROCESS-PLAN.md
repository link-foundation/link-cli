# Author takeover across process exit

Advance R1/R4/R7/R8/R9 beyond clean-close native handoffs. A lost mock reply
does not establish that an actual persistent ownership claim survives process
exit. Keep the complete five-runtime matrix in CI and run focused controls
locally with finite inputs and memory limits.

- [x] Preserve a missing-recipe baseline before implementing process actions.
- [x] Extend the existing native Rust, C# and JS physical recipes with exits
      immediately before and after the takeover CAS, without disposal/cleanup.
- [x] Reap each child, require ordinary opens to reject its ownership marker
      without modifying any physical file, then preserve markers separately
      during explicit operator recovery.
- [x] Require exact original predecessor/token retry after either outcome;
      retained durable retry must leave every physical file hash unchanged.
- [x] Verify that takeover and data publication are separate decisions: a
      committed takeover can retain the previous data until the next guarded
      write. Fence every superseded owner and publish a following third owner.
- [x] Run a focused local process recipe and compile/format the native ports;
      wire all 20 exits and 100 cross-runtime recovery checks to complete CI.
- [ ] Preserve fresh exact-head CI and continue all wider unfinished criteria.

These controls verify application process exits at selected provider boundaries.
They do not establish arbitrary torn-write or hardware power-loss certification,
automatic recovery authority, bypass-writer admission or receipt pruning.
