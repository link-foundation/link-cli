---
bump: minor
---

Retain checked mapped-view flush ownership through Rust storage decorators and
explicit generic adoption. On Windows, synchronize the current mapped view
before its owned file buffers. Reject failed remaps and retired handles, and
release retained file handles when the mapping closes. This barrier does not
supply physical torn-page repair or hardware power-loss certification.
