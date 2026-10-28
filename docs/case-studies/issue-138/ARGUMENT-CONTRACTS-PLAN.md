# Public Rust argument contracts

Requirement mapping: R3/R6/R7/R8/R9/R12. Keep all complete-project measured
files and every wider unfinished criterion in the acceptance inventory.

- [x] Read the public parser, documented options and existing argument tests.
- [x] Compare inline and separate values through all documented long aliases;
      preserve Unicode, whitespace and additional delimiters exactly.
- [x] Check explicit boolean spellings, defaults, subsequent positional input,
      disabled strategy selection and independent decorator selection.
- [x] Verify unsigned link/signed sequence boundaries, arity validation,
      missing values, unknown options and literal input after --.
- [x] Check for defects before changing production behavior; none reproduced.
      An initial test oracle was corrected to the established canonical alias.
- [x] Run the same portable argument contracts in the actual WASM harness,
      retaining the verified 256 MiB linear-memory cap and finite test deadlines.
- [x] Run focused contracts, strict Clippy/format, complete native coverage and
      repository/release checks; retain exact reports and an atomic commit.
- [x] Obtain fresh exact-head CI, including all supported native platforms,
      and continue every wider unfinished requirement.

The preceding complete native report measures cli.rs at 352/405 lines and
100/118 reported branches. Public semantic controls take priority over merely
executing a branch: compare the entire parsed configuration with independent
expected fields, and verify that errors do not change a subsequent parse.
String values remain uninterpreted at this boundary; database and retention
validation belongs to command execution. Boolean literals have established
optional-value behavior, so an unrecognized following token remains a query.

All seven groups pass natively and in actual WASM. Complete suites pass 930
native cases and 546 WASM cases. The parser reaches 404/405 native lines and
394/405 WASM lines; complete production remains below 100%. See arguments.txt
and VERIFICATION.md for complete denominators. No production source changed.
All nine workflows pass on 164f6f0, including the native platform matrices.
arguments-ci.txt preserves exact SHA, creation timestamps and links.
