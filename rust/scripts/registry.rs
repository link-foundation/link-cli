//! crates.io, Docker Hub and GitHub lookups shared by
//! `check-release-needed.rs` and `version-and-commit.rs` (issue #134).
//!
//! Every lookup retries network failures, 429 and 5xx with backoff and
//! returns an error once its attempts run out. The release scripts used to
//! read any failed request as "not published": one dropped connection made
//! `check-release-needed.rs` release a published version again, and
//! `version-and-commit.rs` bump from Cargo.toml instead of the highest
//! published version. Both ended in a misleading "VERSION ALREADY PUBLISHED"
//! error from `publish-crate.rs`.
//!
//! Included with `#[path = "registry.rs"] mod registry;`;
//! its tests run through `rust-script --test rust/scripts/version-and-commit.rs`.

#![allow(dead_code)]

use serde::Deserialize;
use std::thread::sleep;
use std::time::Duration;

const ATTEMPTS: u32 = 4;
const FIRST_DELAY: Duration = Duration::from_secs(1);

/// Why one attempt failed: a transient failure is worth another attempt,
/// a permanent one (such as 403) is not.
pub enum Failure {
    Transient(String),
    Permanent(String),
}

/// Run `operation` until it succeeds, waiting `delay`, then twice as long,
/// between attempts.
pub fn with_retries<T>(
    attempts: u32,
    delay: Duration,
    mut operation: impl FnMut() -> Result<T, Failure>,
) -> Result<T, String> {
    let mut attempt = 1;
    loop {
        match operation() {
            Ok(value) => return Ok(value),
            Err(Failure::Permanent(message)) => return Err(message),
            Err(Failure::Transient(message)) if attempt >= attempts => {
                return Err(format!("{} (after {} attempts)", message, attempt));
            }
            Err(Failure::Transient(message)) => {
                println!(
                    "Retrying after: {} (attempt {} of {})",
                    message,
                    attempt + 1,
                    attempts
                );
                sleep(delay * 2u32.pow(attempt - 1));
                attempt += 1;
            }
        }
    }
}

/// GET `url` and return its body; `None` when it answers 404.
pub fn get_body(url: &str, headers: &[(&str, &str)]) -> Result<Option<String>, String> {
    with_retries(ATTEMPTS, FIRST_DELAY, || {
        let mut request = ureq::get(url).set("User-Agent", "link-cli-release-scripts");
        for (name, value) in headers {
            request = request.set(name, value);
        }
        match request.call() {
            Ok(response) => response
                .into_string()
                .map(Some)
                .map_err(|e| Failure::Transient(format!("{}: {}", url, e))),
            Err(ureq::Error::Status(404, _)) => Ok(None),
            Err(ureq::Error::Status(code, _)) if code == 429 || code >= 500 => {
                Err(Failure::Transient(format!("{} answered {}", url, code)))
            }
            Err(ureq::Error::Status(code, _)) => {
                Err(Failure::Permanent(format!("{} answered {}", url, code)))
            }
            Err(e) => Err(Failure::Transient(e.to_string())),
        }
    })
}

#[derive(Deserialize)]
struct CratesIoCrate {
    versions: Option<Vec<CratesIoVersionEntry>>,
}

#[derive(Deserialize)]
struct CratesIoVersionEntry {
    num: String,
    yanked: bool,
}

fn semver_core(version: &str) -> Option<(u32, u32, u32)> {
    let parts: Vec<&str> = version.split('-').next()?.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    Some((
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
    ))
}

/// The highest non-yanked `x.y.z` in a `GET /api/v1/crates/{name}` body.
pub fn parse_max_version(body: &str) -> Result<Option<(u32, u32, u32)>, String> {
    let data: CratesIoCrate =
        serde_json::from_str(body).map_err(|e| format!("Unexpected crates.io response: {}", e))?;
    Ok(data
        .versions
        .unwrap_or_default()
        .iter()
        .filter(|v| !v.yanked)
        .filter_map(|v| semver_core(&v.num))
        .max())
}

/// The highest published version of `crate_name`; `None` before its first release.
pub fn max_published_version(crate_name: &str) -> Result<Option<(u32, u32, u32)>, String> {
    match get_body(
        &format!("https://crates.io/api/v1/crates/{}", crate_name),
        &[],
    )? {
        Some(body) => parse_max_version(&body),
        None => Ok(None),
    }
}

/// Whether `crate_name@version` is on crates.io.
pub fn version_published(crate_name: &str, version: &str) -> Result<bool, String> {
    let url = format!("https://crates.io/api/v1/crates/{}/{}", crate_name, version);
    Ok(get_body(&url, &[])?.is_some())
}

/// Whether `repository` has a GitHub release for `tag`; uses GITHUB_TOKEN when set.
pub fn github_release_exists(repository: &str, tag: &str) -> Result<bool, String> {
    let url = format!(
        "https://api.github.com/repos/{}/releases/tags/{}",
        repository, tag
    );
    let token = std::env::var("GITHUB_TOKEN").unwrap_or_default();
    let authorization = format!("Bearer {}", token);
    let mut headers = vec![("Accept", "application/vnd.github+json")];
    if !token.is_empty() {
        headers.push(("Authorization", authorization.as_str()));
    }
    Ok(get_body(&url, &headers)?.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn transient_failures_are_retried_until_success() {
        let calls = Cell::new(0);
        let result = with_retries(4, Duration::ZERO, || {
            calls.set(calls.get() + 1);
            if calls.get() < 3 {
                Err(Failure::Transient(
                    "https://crates.io answered 503".to_string(),
                ))
            } else {
                Ok("1.0.8")
            }
        });
        assert_eq!(result, Ok("1.0.8"));
        assert_eq!(calls.get(), 3);
    }

    #[test]
    fn exhausted_retries_report_the_last_failure_and_the_attempts() {
        let calls = Cell::new(0);
        let result: Result<(), String> = with_retries(2, Duration::ZERO, || {
            calls.set(calls.get() + 1);
            Err(Failure::Transient(
                "crates.io: Connection Failed".to_string(),
            ))
        });
        assert_eq!(
            result,
            Err("crates.io: Connection Failed (after 2 attempts)".to_string())
        );
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn permanent_failures_are_not_retried() {
        let calls = Cell::new(0);
        let result: Result<(), String> = with_retries(4, Duration::ZERO, || {
            calls.set(calls.get() + 1);
            Err(Failure::Permanent("answered 403".to_string()))
        });
        assert_eq!(result, Err("answered 403".to_string()));
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn max_version_skips_yanked_and_compares_numerically() {
        let body = r#"{"versions":[
            {"num":"1.0.9","yanked":true},
            {"num":"1.0.10","yanked":false},
            {"num":"1.0.8","yanked":false},
            {"num":"1.1.0-rc.1","yanked":false}
        ]}"#;
        assert_eq!(parse_max_version(body), Ok(Some((1, 1, 0))));
        assert_eq!(parse_max_version(r#"{"versions":[]}"#), Ok(None));
        assert!(parse_max_version("<html>").is_err());
    }
}
