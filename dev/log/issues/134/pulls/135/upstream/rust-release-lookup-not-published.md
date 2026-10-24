## Summary

`scripts/check-release-needed.rs` and `scripts/version-and-commit.rs` read **any failed crates.io, Docker Hub or GitHub request as "not published"**. Nothing retries them. One dropped connection or one 5xx/429 answer is enough to:

- make `check-release-needed.rs` answer `should_release=true` and `skip_bump=true` for a version that is fully published. The release job then publishes it again, and `publish-crate.rs` fails with `=== VERSION ALREADY PUBLISHED === … This indicates a bug in version bumping`, which points at the wrong cause;
- make `version-and-commit.rs` bump from `Cargo.toml` instead of the highest published version (`get_max_published_version` → `None`), and skip the "already on crates.io" step of `ensure_version_exceeds_published` (`check_version_on_crates_io` → `false`).

Code (template `main` at 265219d):

- `check-release-needed.rs`: [`check_version_on_crates_io`](https://github.com/link-foundation/rust-ai-driven-development-pipeline-template/blob/265219d098ec1c6acbe6749562c2f3dc057f0df8/scripts/check-release-needed.rs#L96-L118), [`check_docker_hub_tag`](https://github.com/link-foundation/rust-ai-driven-development-pipeline-template/blob/265219d098ec1c6acbe6749562c2f3dc057f0df8/scripts/check-release-needed.rs#L133-L158), [`check_github_release`](https://github.com/link-foundation/rust-ai-driven-development-pipeline-template/blob/265219d098ec1c6acbe6749562c2f3dc057f0df8/scripts/check-release-needed.rs#L160-L185) and [`get_max_published_version`](https://github.com/link-foundation/rust-ai-driven-development-pipeline-template/blob/265219d098ec1c6acbe6749562c2f3dc057f0df8/scripts/check-release-needed.rs#L215-L264)

  ```rust
  Err(ureq::Error::Status(404, _)) => false,
  Err(e) => {
      eprintln!("Warning: Could not check crates.io: {}", e);
      false
  }
  ```

- `version-and-commit.rs`: [`check_version_on_crates_io` and `get_max_published_version`](https://github.com/link-foundation/rust-ai-driven-development-pipeline-template/blob/265219d098ec1c6acbe6749562c2f3dc057f0df8/scripts/version-and-commit.rs#L285-L341), which return `Err(_) => false` and `Err(_) => None` respectively.

## Reproduction

The script below copies `scripts/` from a template checkout and points the crates.io and GitHub URLs at a local fake registry. That registry serves `link-cli` 1.0.8, which is published and has its GitHub release, and closes the first request for `/api/v1/crates/link-cli/1.0.8` without answering:

```bash
REPRO_REPO=/path/to/template-clone REPRO_SCRIPTS=scripts \
  dev/log/issues/134/pulls/135/scripts/repro-rust-registry-transient.sh HEAD
```

Output against template 265219d (abridged):

```
Max published version on crates.io: 1.0.8
Warning: Could not check crates.io: http://127.0.0.1:PORT/api/v1/crates/link-cli/1.0.8: Network Error: Unexpected EOF
Output: crate_published=false
Output: github_release_published=true
No changelog fragments but v1.0.8 is missing at least one release artifact
Output: should_release=true
Output: skip_bump=true
exit=0
== fake registry requests
GET /api/v1/crates/link-cli
GET /api/v1/crates/link-cli/1.0.8
  -> connection dropped
GET /repos/link-foundation/link-cli/releases/tags/rust-v1.0.8
```

The script needs only `rust-script` and `python3`. It lives in link-foundation/link-cli (issue 134, pull request 135) at `dev/log/issues/134/pulls/135/scripts/repro-rust-registry-transient.sh`. The full outputs are in `dev/log/issues/134/pulls/135/analysis/template-rust-registry-transient-repro.txt` (template) and `rust-registry-transient-repro.txt` (link-cli before and after the fix). The latter also covers a registry that never answers.

## Workaround

Re-run the release job. A re-run after a wrong `skip_bump=true` decision still fails in `publish-crate.rs`, so wait until the registry answers again.

## Suggested fix

Share one lookup helper between both scripts, as link-cli does in `rust/scripts/registry.rs` (included with `#[path = "registry.rs"] mod registry;`):

1. Retry transport errors, 429 and 5xx with exponential backoff, for example 4 attempts with 1 s, 2 s and 4 s waits.
2. Treat 404 as "not found" and any other status as a permanent error.
3. Return `Result<_, String>` and **fail the job** when the attempts run out, instead of guessing:

```rust
pub enum Failure { Transient(String), Permanent(String) }

pub fn with_retries<T>(attempts: u32, delay: Duration,
                       mut operation: impl FnMut() -> Result<T, Failure>) -> Result<T, String> {
    let mut attempt = 1;
    loop {
        match operation() {
            Ok(value) => return Ok(value),
            Err(Failure::Permanent(message)) => return Err(message),
            Err(Failure::Transient(message)) if attempt >= attempts =>
                return Err(format!("{} (after {} attempts)", message, attempt)),
            Err(Failure::Transient(message)) => {
                println!("Retrying after: {} (attempt {} of {})", message, attempt + 1, attempts);
                sleep(delay * 2u32.pow(attempt - 1));
                attempt += 1;
            }
        }
    }
}

/// GET `url` and return its body; `None` when it answers 404.
pub fn get_body(url: &str, headers: &[(&str, &str)]) -> Result<Option<String>, String> {
    with_retries(4, Duration::from_secs(1), || {
        /* ureq::get(url)…call():
           Ok → Some(body); Status(404) → None;
           Status(429 | 5xx) and transport errors → Transient;
           other statuses → Permanent */
    })
}
```

The callers then stop with an annotation that names the lookup:

```rust
fn or_exit<T>(lookup: &str, result: Result<T, String>) -> T {
    result.unwrap_or_else(|e| {
        eprintln!("::error title={} failed::{}", lookup, e);
        exit(1);
    })
}
```

Unit tests can drive `with_retries` with closures (retried until success, attempts exhausted, permanent error not retried) and parse the version list without network access. link-cli has these four tests in `rust/scripts/registry.rs`.
