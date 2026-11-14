# Cross-language parity

link-cli has three implementations: C# in `csharp/`, Rust in `rust/`, and
JavaScript in `js/`. Before issue #111, nothing in CI noticed when a feature
changed in one of them and not in the others. Two files now make parity
checkable, and the JavaScript stage checks them before any Rust or C# job
starts (see [CI stages](ci-cd/troubleshooting.md#stages)).

| File                          | What it records                                                                                |
| ----------------------------- | ---------------------------------------------------------------------------------------------- |
| `parity/features.json`        | Every CLI and library feature, with its status, sources and tests in C#, Rust and JavaScript.  |
| `parity/golden-fixtures.json` | The shared `docs/protocol` fixtures that both the C# and the Rust test suites must load.       |
| `parity/check-parity.mjs`     | The checker. It has no dependencies, and its header lists every command.                       |

## The feature manifest

Each entry in `features[]` has an `id`, a `title`, and one cell per language.

| `status`         | Meaning                                                   | Required fields                                                   |
| ---------------- | --------------------------------------------------------- | ----------------------------------------------------------------- |
| `implemented`    | The language has its own implementation.                  | `sources` and `tests` (globs), plus the `fingerprint` the checker writes |
| `delegated`      | Another language's code does the work.                    | `to` (the language that implements it), and `via` (the binding globs) |
| `not-applicable` | The feature has no meaning in this language.              | `reason`                                                          |
| `missing`        | The feature should exist here but does not yet.           | `issue` (a tracking issue URL)                                    |

Each language also lists `roots`: globs of its source files. A source file
under a root must belong to a feature, or to a `shared` entry with a reason.
This rule is what makes a new file count against the manifest instead of
slipping past it.

## What the checker rejects

`node parity/check-parity.mjs` fails, and lists every finding, when any of
these holds:

1. The manifest breaks the schema: an unknown status, or a missing `tests`,
   `issue`, `to` or `reason`.
2. A glob matches no tracked file, for example because a file was renamed.
3. A source file under a language root belongs to no feature.
4. A recorded fingerprint is stale: the code changed and the manifest did not.
5. **One-sided change.** Compared with the base commit, only some of a
   feature's `implemented` cells changed, and no `parityExceptions` entry
   acknowledges exactly those new fingerprints.
6. `parity/golden-fixtures.json` disagrees with the fixture files. This covers
   a hash mismatch, a fixture missing from the manifest, a fixture with no C#
   or Rust consumer test, and a consumer test that no longer mentions its
   fixture.
7. With `--receipt`: a test suite did not report loading every golden fixture
   with the expected hash.

The JavaScript stage passes the base commit as `PARITY_BASE_SHA`. On a pull
request that is the pull request's base; on a push it is the commit before
the push. The stage checks out with `fetch-depth: 0`. If that commit is
missing from the clone, the checker fails. It never skips rule 5 silently.

## Daily use

```bash
# Check the working tree against main, as CI does on a pull request.
node parity/check-parity.mjs --base origin/main

# Print every matched file and fingerprint.
node parity/check-parity.mjs --base origin/main --verbose   # or PARITY_DEBUG=1

# After changing code, refresh the fingerprints and fixture hashes.
node parity/check-parity.mjs --update

# A change that is deliberately one-sided, such as a Rust-only performance fix.
node parity/check-parity.mjs --base origin/main --acknowledge <feature-id> \
  --reason "Rust-only performance fix; behaviour is unchanged" \
  --ref https://github.com/link-foundation/link-cli/pull/<n>
```

An acknowledgement covers only the fingerprints it records. If the same
feature changes again on one side, the checker needs a new acknowledgement.

When you add a feature:

1. Add its entry to `parity/features.json`, with an honest status for each
   language.
2. Run `--update`.

A `missing` cell links the issue that tracks the gap.

## Golden fixtures

Golden fixtures are the files under `docs/protocol/` that match
`include` in `parity/golden-fixtures.json`. Both suites have to load them:

- Rust: `rust/tests/golden_fixtures_tests.rs`
- C#: `csharp/Foundation.Data.Doublets.Cli.Tests/GoldenFixturesTests.cs`

Each test reads every listed fixture and checks its SHA-256. When the
`GOLDEN_FIXTURES_RECEIPT` variable is set, the test also writes a receipt,
`{ "suite", "fixtures": { path: sha256 } }`. In `rust.yml` and `csharp.yml`,
the step after the tests runs `node parity/check-parity.mjs --receipt <suite>
<file>`. If a suite filtered the test out, skipped it, or was not built, the
receipt is missing or incomplete and CI fails.

To change a fixture:

1. Edit the file.
2. Run `node parity/check-parity.mjs --update`.
3. Make both suites pass.

To add a fixture, list it with at least one C# and one Rust consumer test
that mentions its file name.

## Related

- [JS-WORKBENCH.md](JS-WORKBENCH.md) lists what `js/` implements itself and
  what it delegates to the Rust WebAssembly build.
- [ci-cd/troubleshooting.md](ci-cd/troubleshooting.md) explains how to read a
  failed parity check in CI.
- [case-studies/issue-111](case-studies/issue-111/README.md) records why the
  gate exists.
