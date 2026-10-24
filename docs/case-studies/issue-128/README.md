# Lossless C# LiNo name imports

[Issue #128](https://github.com/link-foundation/link-cli/issues/128) reports that
importing `(a: a 'x\y')` stores `xy` rather than `x\y` in clink 4.0.2.
[PR #131](https://github.com/link-foundation/link-cli/pull/131) fixes the importer
and exporter together.

## Root cause and chosen syntax

`LinoDatabaseInput.ReadQuotedReference` consumed a backslash and appended only
the following character. This silently removed unknown escapes, changed Windows
paths, and treated a trailing backslash as an escape of the closing quote.
`ParseDefinitions` also split on every physical LF before reading quotes, so
an embedded newline could not survive import. The exporter emitted bare
backslashes, physical newlines, and backslash-escaped single quotes for names
containing both quote kinds.

The issue permits keeping backslashes literal or documenting a strict escape
syntax. This fix chooses literal backslashes, matching the existing notation
library. Single, double, and backtick delimiters enclose literal names; choose
a delimiter absent from the content. `\n` stays a backslash and `n`. Physical
LF and CRLF inside quoted names are preserved exactly. Definitions still require
exactly an index and two references, one definition per logical line.

The notation library already exposes `ReferenceLiteral.Encode` and `Decode`.
The exporter uses their `~1{UTF-8 hex}` form for control characters and mixed
quotes. These definitions occupy a single physical line. The importer retains
the distinction between a bare numeric address and a quoted or encoded numeric
name. It also quotes literal-looking names such as `~1{31}` during export, so
they cannot accidentally be decoded on the next import.

Previously generated mixed-quote exports containing `\'` should use another
delimiter or a reference literal instead. Backslash escaping is deliberately
replaced by the issue's literal-backslash interpretation.

## Requirements and regression evidence

| Requirement | Implementation and verification |
| --- | --- |
| Preserve the reported `x\y` name | `ImportText_PreservesBackslashInIssueReproduction` checks the stored target name and absence of `xy`. |
| Preserve names in all reference positions | `ImportText_PreservesLiteralQuotedNames` verifies index, source, and target, including trailing backslashes, UNC paths, and literal `\n\r\t`. |
| Preserve newlines and mixed quotes | Unit tests cover physical LF/CRLF, backtick-quoted mixed quotes, and UTF-8 reference literals. |
| Export a safe representation | `FormatDatabase_WritesLosslessNames` checks backslash quoting, literal encoding, Unicode whitespace, and literal-looking names. |
| Import, export, import into a fresh store and compare | Six `ImportExport_RoundTripsUserTextIntoFreshDatabase` CLI cases check the original stored name, then compare complete binary store archives byte for byte. |
| Preserve existing numeric-name behavior | Existing numeric-name unit and CLI archive round-trip regressions remain in the suite; encoded numeric names have an additional regression. |
| Prepare a package release | `csharp/.changeset/lossless-lino-import.md` requests a patch release of both C# packages through the existing release automation. |

Before changing production code, the focused suite failed 25 of 53 cases. The
failures included the original backslash loss, unterminated names ending in
backslash, multiline names, literal decoding, and unsafe export formatting.

## Initial CI failure

The Dependencies workflow on the prepared commit
`8bac6f03b91dd1acf8924e7b7c66629cb95e9ef3` ran at 2026-10-24 06:31:16 UTC.
[Run 40070532481](https://github.com/link-foundation/link-cli/actions/runs/40070532481)
failed because seven dependency entries were stale. Downloaded log lines 351–357
identify the notation library in C# and its example (0.26.0 → 0.27.1), the Rust
notation crate (0.26.0 → 0.27.1), and Lucide (1.57.0 → 1.61.0) and Vite
(8.3.4 → 8.3.5) in the JavaScript manifest and lockfile. Dependency updates and
lockfiles are included to satisfy the repository's existing freshness policy.

Local command output and downloaded CI logs are retained in the ignored
`ci-logs/` directory beside this document. The PR description records final
validation and CI links.
