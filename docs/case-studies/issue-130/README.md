# Quoted numeric query references

Issue: https://github.com/link-foundation/link-cli/issues/130

## Reproduction and cause

On the original code, seed two named points and then use numeric names:

```sh
clink --db q.links '() ((a: a a) (b: b b))'
clink --db q.links --auto-create-missing-references '() ((p: x "1"))'
clink --db q.links --auto-create-missing-references "() ((p2: x '2'))"
clink --db q.links --auto-create-missing-references '() ((z: x "0"))'
```

The original processors resolved `"1"` and `'2'` as addresses 1 and 2 and rejected
`"0"` as an unsupported address. A name such as `"99"` changed meaning once
address 99 existed. The notation parsers expose string identifiers without the
original delimiters; the query processors then inferred address, variable, and
wildcard semantics from those strings.

The initial regression tests were run against the original production code
before implementing the fix. All nine C# cases and all three Rust cases failed:

- [C# baseline](evidence/csharp-before.txt): zero was rejected, numeric names were
  absent, and strict validation accepted address 1 without the required name.
- [Rust baseline](evidence/rust-before.txt): the same address/name confusion.

Further regression tests exposed quoting loss in stored trigger normalization
in both implementations and unquoted numeric names in Rust's text exporter.
Each failed before its corresponding fix.

## Implementation

A query-specific parser retains reference kinds while using the existing
notation library for syntax validation, comments, and reference literals.
All names carry an internal quote envelope through pattern planning; only name
lookups and writes remove it. Enveloping every name also distinguishes names
that themselves contain quotes. Bare numeric addresses, variables, and wildcards
retain their existing syntax. Original-source validation preserves diagnostic
line and column locations.

Single quotes, double quotes, backticks, N-delimiter quoting, literal backslashes,
multiline values, and `~1{hex}` reference literals keep their notation semantics.
Quoted or encoded `*` and `$value` are names. Strict validation requires a named
link to exist or be defined in the substitution; automatic creation makes a
missing name a point link. Definitions, matching, updates, and deletions use the
same distinction.

Stored trigger normalization uses quoting and reference literals so replaying a
trigger preserves its meaning, while retaining ordinary names' existing stored
text for trigger lookup and removal. Rust's text exporters quote numeric
names, matching the C# exporter introduced for issue #112. Generic notation
parsing and numeric address syntax remain unchanged.
The public Rust reference validator continues to accept already-decoded names;
only the query processor opts into interpreting the internal envelopes.

## Verification

The regression suites cover:

| Requirement | Automated evidence |
| --- | --- |
| Quoted `0`, `1`, `2`, `99`, leading zeros, `+1`, and maximum `uint` | C# `QuotedNumbersAreNames`; Rust `quoted_numbers_are_names` |
| Single, double, and backtick quotes; encoded names | Quoted-number and quoted/encoded-reference tests in both engines |
| Stable name when address 99 is allocated later | `NumericNameRemainsStableWhenItsAddressIsAllocatedLater` and Rust equivalent |
| Bare numeric addresses retain their meaning | Assertions alongside quoted-name cases and CLI tests |
| Strict references and substitution definitions | Strict missing-name and definition/mutation tests in both engines |
| Matching, updates, and deletion by numeric name | `QuotedDefinitionsSupportStrictMatchingUpdatingAndDeletion` and Rust equivalent |
| Exported names replay as queries | Library export/replay tests and CLI export/query round trips in both engines |
| Stored query normalization preserves kinds | Normalized-trigger query tests in both engines |
| Existing trigger identity remains compatible | `TriggerNormalizationPreservesExistingNamedQueryText` and Rust equivalent |
| Public Rust validation retains literal quotes in parsed names | `public_validator_preserves_literal_quotes_in_parsed_names` |
| Existing notation syntax and envelope collisions | Delimiter, unmatched quote-prefix, comment, backslash, Unicode, control, and literal-name tests |
| WebAssembly uses the same semantics | `quoted_numbers_stay_names_in_the_javascript_api` in the WASM test suite |

The [reusable example](../../../examples/quoted-query-names/README.md) also checks
the issue's queries and execution record values (`exitCode "0"`, `pid "12345"`)
in separate processes. Both implementations produce identical results:
[C#](evidence/csharp-example.txt), [Rust](evidence/rust-example.txt).

Local validation uses sequential Rust builds with one compiler job and debug
symbols disabled to stay within the container's memory budget. C# runs use a
512 MiB managed heap limit and one MSBuild worker.

- C#: 552 tests passed after merging main's import validation fix; formatting
  and file-size checks passed.
- Rust core: 370 tests passed across 41 suites; two existing tests ignored.
  All-target, all-feature Clippy and formatting passed.
- JavaScript, client, release-script, and workflow tests: 147 passed.
- WebAssembly wrapper: seven tests passed under Node, including numeric names
  through the JavaScript API; all-target, all-feature Clippy passed.
- The optimized WebAssembly and React browser build passed.
- Client type declarations and conformance on Node.js, Bun, and Deno passed:
  32 scenarios per runtime, with four expected WASM-only skips per runtime.
  The 40-record synthetic consumer also read back identical results from all
  three backends. [Local check summary](evidence/local-checks.txt).

Patch release fragments are included for both published implementations. The
issue has no required sub-issues or additional issue/PR review requirements.
