#!/usr/bin/env rust-script
//! Bump version in Cargo.toml and commit changes
//! Used by the CI/CD pipeline for releases
//!
//! IMPORTANT: This script checks crates.io (the source of truth for Rust packages),
//! NOT git tags, which can exist before a package is published.
//!
//! Supports both single-language and multi-language repository structures:
//! - Single-language: Cargo.toml and changelog.d/ in repository root
//! - Multi-language: Cargo.toml and changelog.d/ in rust/ subfolder
//!
//! Usage: rust-script scripts/version-and-commit.rs --bump-type <major|minor|patch> [--description <desc>] [--rust-root <path>] [--tag-prefix <prefix>] [--release-label <label>]
//!
//! ```cargo
//! [dependencies]
//! regex = "1"
//! chrono = "0.4"
//! ureq = "2"
//! serde = { version = "1", features = ["derive"] }
//! serde_json = "1"
//! ```

// Test harnesses replace main; the real build still denies unused helpers.
#![cfg_attr(test, allow(dead_code))]

#[cfg(not(test))]
use chrono::Utc;
use regex::Regex;
#[cfg(not(test))]
use serde::Deserialize;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
#[cfg(not(test))]
use std::process::exit;
use std::process::Command;

#[path = "release-index.rs"]
mod release_index;

#[path = "release-naming.rs"]
mod release_naming;
#[path = "rust-paths.rs"]
mod rust_paths;

fn get_arg(name: &str) -> Option<String> {
    let args: Vec<String> = env::args().collect();
    let flag = format!("--{}", name);

    if let Some(idx) = args.iter().position(|a| a == &flag) {
        return args.get(idx + 1).cloned();
    }

    let env_name = name.to_uppercase().replace('-', "_");
    env::var(&env_name).ok().filter(|s| !s.is_empty())
}

fn get_changelog_dir(rust_root: &str) -> String {
    if rust_root == "." {
        "./changelog.d".to_string()
    } else {
        format!("{}/changelog.d", rust_root)
    }
}

fn get_changelog_path(rust_root: &str) -> String {
    if rust_root == "." {
        "./CHANGELOG.md".to_string()
    } else {
        format!("{}/CHANGELOG.md", rust_root)
    }
}

#[path = "changelog-files.rs"]
mod changelog_files;

#[path = "github-output.rs"]
mod github_output;

fn set_output(key: &str, value: &str) -> std::io::Result<()> {
    github_output::write_output(key, value)?;
    println!("Output: {key}={value}");
    Ok(())
}

fn exec(command: &str, args: &[&str]) -> Result<String, String> {
    match Command::new(command).args(args).output() {
        Ok(output) => {
            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                // Both streams: git writes push rejections to stderr but some
                // helpers log to stdout, and classifying a failure (issue #162)
                // must see everything the command printed.
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(format!(
                    "Command `{}` failed.\nstdout: {}\nstderr: {}",
                    command,
                    stdout.trim(),
                    stderr.trim()
                ))
            }
        }
        Err(e) => Err(format!("Failed to execute: {}", e)),
    }
}

fn exec_check(command: &str, args: &[&str]) -> bool {
    Command::new(command)
        .args(args)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Not every failed `git push` is a lost race (issue #162). A repository
/// ruleset rejection arrives shaped like a non-fast-forward, and rebasing
/// against a policy refusal three times burns the job's budget before dying
/// with a misleading "conflict" error, so the failure is classified first.
const REPOSITORY_RULE_PATTERNS: &[&str] = &[
    "gh006",
    "gh013",
    "repository rule violations",
    "changes must be made through a pull request",
    "protected branch",
    "push declined",
];

const NON_FAST_FORWARD_PATTERNS: &[&str] = &[
    "[rejected]",
    "non-fast-forward",
    "fetch first",
    "updates were rejected",
];

#[derive(Debug, PartialEq, Eq)]
enum PushFailure {
    /// A repository ruleset refuses the push; retrying cannot fix policy.
    RepositoryRules,
    /// The remote branch moved; rebase onto it and try again.
    LostRace,
    /// Anything else: report the real error instead of masking it.
    Other,
}

fn classify_push_failure(raw_output: &str) -> PushFailure {
    let haystack = raw_output.to_lowercase();
    // Rules first: a ruleset rejection also contains the word "rejected".
    if REPOSITORY_RULE_PATTERNS
        .iter()
        .any(|pattern| haystack.contains(pattern))
    {
        return PushFailure::RepositoryRules;
    }
    if NON_FAST_FORWARD_PATTERNS
        .iter()
        .any(|pattern| haystack.contains(pattern))
    {
        return PushFailure::LostRace;
    }
    PushFailure::Other
}

struct Version {
    major: u32,
    minor: u32,
    patch: u32,
    #[allow(dead_code)]
    pre_release: Option<String>,
}

impl Version {
    fn parse(content: &str) -> Option<Version> {
        let re = Regex::new(r#"(?m)^version\s*=\s*"(\d+)\.(\d+)\.(\d+)(?:-([^"]+))?""#).ok()?;
        let caps = re.captures(content)?;
        Some(Version {
            major: caps.get(1)?.as_str().parse().ok()?,
            minor: caps.get(2)?.as_str().parse().ok()?,
            patch: caps.get(3)?.as_str().parse().ok()?,
            pre_release: caps.get(4).map(|m| m.as_str().to_string()),
        })
    }

    fn bump(&self, bump_type: &str) -> String {
        match bump_type {
            "major" => format!("{}.0.0", self.major + 1),
            "minor" => format!("{}.{}.0", self.major, self.minor + 1),
            _ => format!("{}.{}.{}", self.major, self.minor, self.patch + 1),
        }
    }
}

fn update_cargo_toml(cargo_toml_path: &str, new_version: &str) -> Result<(), String> {
    let content = fs::read_to_string(cargo_toml_path)
        .map_err(|e| format!("Failed to read {}: {}", cargo_toml_path, e))?;

    let re = Regex::new(r#"(?m)^(version\s*=\s*")[^"]+(")"#).unwrap();
    let new_content = re.replace(&content, format!("${{1}}{}${{2}}", new_version).as_str());

    fs::write(cargo_toml_path, new_content.as_ref())
        .map_err(|e| format!("Failed to write {}: {}", cargo_toml_path, e))?;

    println!("Updated {} to version {}", cargo_toml_path, new_version);
    Ok(())
}

fn update_cargo_lock(
    cargo_lock_path: &Path,
    crate_name: &str,
    new_version: &str,
) -> Result<bool, String> {
    if !cargo_lock_path.exists() {
        println!(
            "No Cargo.lock at {} (skipping lock-file version sync)",
            cargo_lock_path.display()
        );
        return Ok(false);
    }

    let path_str = cargo_lock_path.to_string_lossy();
    let content = fs::read_to_string(cargo_lock_path)
        .map_err(|e| format!("Failed to read {}: {}", path_str, e))?;

    let pattern = format!(
        r#"(?m)(\[\[package\]\]\s*\nname\s*=\s*"{}"\s*\nversion\s*=\s*")[^"]+(")"#,
        regex::escape(crate_name),
    );
    let re =
        Regex::new(&pattern).map_err(|e| format!("Failed to build Cargo.lock regex: {}", e))?;

    if !re.is_match(&content) {
        println!(
            "Warning: Could not find [[package]] entry for `{}` in {} (lock file left untouched)",
            crate_name, path_str
        );
        return Ok(false);
    }

    let new_content = re.replace(&content, format!("${{1}}{}${{2}}", new_version).as_str());

    if new_content == content {
        println!("Cargo.lock already at version {}", new_version);
        return Ok(false);
    }

    fs::write(cargo_lock_path, new_content.as_ref())
        .map_err(|e| format!("Failed to write {}: {}", path_str, e))?;

    println!("Updated {} to version {}", path_str, new_version);
    Ok(true)
}

#[cfg(not(test))]
#[derive(Deserialize)]
struct CratesIoCrate {
    versions: Option<Vec<CratesIoVersionEntry>>,
}

#[cfg(not(test))]
#[derive(Deserialize)]
struct CratesIoVersionEntry {
    num: String,
    yanked: bool,
}

fn get_crate_name(cargo_toml_path: &str) -> Result<String, String> {
    let content = fs::read_to_string(cargo_toml_path)
        .map_err(|e| format!("Failed to read {}: {}", cargo_toml_path, e))?;

    let re = Regex::new(r#"(?m)^name\s*=\s*"([^"]+)""#).unwrap();

    if let Some(caps) = re.captures(&content) {
        Ok(caps.get(1).unwrap().as_str().to_string())
    } else {
        Err(format!("Could not find name in {}", cargo_toml_path))
    }
}

fn check_tag_exists(tag_prefix: &str, version: &str) -> bool {
    exec_check("git", &["rev-parse", &format!("{}{}", tag_prefix, version)])
}

#[cfg(not(test))]
fn check_version_on_crates_io(crate_name: &str, version: &str) -> bool {
    let url = format!("https://crates.io/api/v1/crates/{}/{}", crate_name, version);
    match ureq::get(&url)
        .set("User-Agent", "rust-script-version-and-commit")
        .call()
    {
        Ok(response) => response.status() == 200,
        Err(_) => false,
    }
}

#[cfg(not(test))]
fn get_max_published_version(crate_name: &str) -> Option<(u32, u32, u32)> {
    let url = format!("https://crates.io/api/v1/crates/{}", crate_name);
    match ureq::get(&url)
        .set("User-Agent", "rust-script-version-and-commit")
        .call()
    {
        Ok(response) => {
            if response.status() == 200 {
                if let Ok(body) = response.into_string() {
                    if let Ok(data) = serde_json::from_str::<CratesIoCrate>(&body) {
                        if let Some(versions) = data.versions {
                            let mut max: Option<(u32, u32, u32)> = None;
                            for v in &versions {
                                if v.yanked {
                                    continue;
                                }
                                let base = match v.num.split('-').next() {
                                    Some(b) => b,
                                    None => continue,
                                };
                                let parts: Vec<&str> = base.split('.').collect();
                                if parts.len() == 3 {
                                    if let (Ok(a), Ok(b), Ok(c)) = (
                                        parts[0].parse::<u32>(),
                                        parts[1].parse::<u32>(),
                                        parts[2].parse::<u32>(),
                                    ) {
                                        let tuple = (a, b, c);
                                        if max.map_or(true, |m| tuple > m) {
                                            max = Some(tuple);
                                        }
                                    }
                                }
                            }
                            return max;
                        }
                    }
                }
            }
            None
        }
        Err(_) => None,
    }
}

#[cfg(not(test))]
fn ensure_version_exceeds_published(
    version_str: &str,
    crate_name: &str,
    tag_prefix: &str,
    max_published: Option<(u32, u32, u32)>,
) -> String {
    let parts: Vec<&str> = version_str
        .split('-')
        .next()
        .unwrap_or(version_str)
        .split('.')
        .collect();
    if parts.len() != 3 {
        return version_str.to_string();
    }

    let mut major: u32 = parts[0].parse().unwrap_or(0);
    let mut minor: u32 = parts[1].parse().unwrap_or(0);
    let mut patch: u32 = parts[2].parse().unwrap_or(0);

    if let Some((pub_major, pub_minor, pub_patch)) = max_published {
        if (major, minor, patch) <= (pub_major, pub_minor, pub_patch) {
            println!(
                "Version {}.{}.{} is not greater than max published {}.{}.{}, adjusting to {}.{}.{}",
                major, minor, patch,
                pub_major, pub_minor, pub_patch,
                pub_major, pub_minor, pub_patch + 1
            );
            major = pub_major;
            minor = pub_minor;
            patch = pub_patch + 1;
        }
    }

    let mut candidate = format!("{}.{}.{}", major, minor, patch);
    let mut safety_counter = 0;
    while (check_tag_exists(tag_prefix, &candidate)
        || check_version_on_crates_io(crate_name, &candidate))
        && safety_counter < 100
    {
        println!(
            "Version {} already has a git tag or is published on crates.io, bumping patch",
            candidate
        );
        patch += 1;
        candidate = format!("{}.{}.{}", major, minor, patch);
        safety_counter += 1;
    }

    if safety_counter >= 100 {
        eprintln!("Error: Could not find an unpublished version after 100 attempts");
        exit(1);
    }

    candidate
}

fn strip_frontmatter(content: &str) -> String {
    let re = Regex::new(r"(?s)^---\s*\n.*?\n---\s*\n(.*)$").unwrap();
    if let Some(caps) = re.captures(content) {
        caps.get(1).unwrap().as_str().trim().to_string()
    } else {
        content.trim().to_string()
    }
}

fn remove_changelog_fragments(files: &[PathBuf]) {
    for file in files {
        fs::remove_file(file)
            .unwrap_or_else(|e| panic!("Failed to remove {}: {}", file.display(), e));
        println!("Removed {}", file.display());
    }
}

#[cfg(not(test))]
fn collect_changelog(changelog_dir: &str, changelog_file: &str, version: &str) {
    let date_str = Utc::now().format("%Y-%m-%d").to_string();
    collect_changelog_with_date(changelog_dir, changelog_file, version, &date_str);
}

fn collect_changelog_with_date(
    changelog_dir: &str,
    changelog_file: &str,
    version: &str,
    date_str: &str,
) {
    let files = changelog_files::list_fragments(Path::new(changelog_dir))
        .unwrap_or_else(|error| panic!("{error}"));
    if files.is_empty() {
        return;
    }

    let fragments: Vec<String> = files
        .iter()
        .map(|file| changelog_files::read_file(file).unwrap_or_else(|error| panic!("{error}")))
        .map(|c| strip_frontmatter(&c))
        .filter(|c| !c.is_empty())
        .collect();

    if fragments.is_empty() {
        return;
    }

    let new_entry = format!(
        "\n## [{}] - {}\n\n{}\n",
        version,
        date_str,
        fragments.join("\n\n")
    );

    if !Path::new(changelog_file).exists() {
        return;
    }

    let mut content = changelog_files::read_file(Path::new(changelog_file))
        .unwrap_or_else(|error| panic!("{error}"));
    let lines: Vec<&str> = content.lines().collect();
    let mut insert_index = None;

    for (i, line) in lines.iter().enumerate() {
        if line.starts_with("## [") {
            insert_index = Some(i);
            break;
        }
    }

    if let Some(idx) = insert_index {
        let mut new_lines: Vec<String> = lines[..idx].iter().map(|s| s.to_string()).collect();
        new_lines.push(new_entry.clone());
        new_lines.extend(lines[idx..].iter().map(|s| s.to_string()));
        content = new_lines.join("\n");
    } else {
        content.push_str(&new_entry);
    }

    fs::write(changelog_file, content).expect("Failed to write changelog");
    remove_changelog_fragments(&files);
    println!("Collected {} changelog fragment(s)", files.len());
}

/// Number of unconsumed changelog fragments (.md files other than README.md).
fn count_changelog_fragments(changelog_dir: &str) -> usize {
    changelog_files::list_fragments(Path::new(changelog_dir))
        .unwrap_or_else(|error| panic!("{error}"))
        .len()
}

/// Verify the release write before it becomes a commit (issue #159).
///
/// The version commit is pushed straight to the default branch by GITHUB_TOKEN,
/// and a push made with GITHUB_TOKEN does not trigger any workflow -- nothing
/// ever lints, formats or tests this commit. Whatever is wrong here ships
/// unreviewed, so the write itself is verified: the changelog actually gained
/// the new version (when fragments were collected), no fragment leaked past
/// this release, and Cargo.lock agrees with the bumped Cargo.toml.
fn verify_release_write(
    changelog_file: &str,
    changelog_dir: &str,
    cargo_lock_path: &Path,
    crate_name: &str,
    new_version: &str,
    expect_new_entry: bool,
) -> Result<(), String> {
    if expect_new_entry {
        let changelog = fs::read_to_string(changelog_file).map_err(|e| {
            format!(
                "CHANGELOG verification failed: cannot read {}: {}",
                changelog_file, e
            )
        })?;
        let entry = Regex::new(&format!(
            r"(?m)^## \[{}\] - \d{{4}}-\d{{2}}-\d{{2}}\s*$",
            regex::escape(new_version)
        ))
        .map_err(|e| format!("Failed to build changelog verification regex: {}", e))?;
        if !entry.is_match(&changelog) {
            return Err(format!(
                "CHANGELOG verification failed: {} does not contain a well-formed \
                 '## [{}] - YYYY-MM-DD' entry after collecting the fragments",
                changelog_file, new_version
            ));
        }
    }

    let leftover: Vec<String> = changelog_files::list_fragments(Path::new(changelog_dir))
        .map_err(|error| format!("CHANGELOG verification failed: {error}"))?
        .iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect();
    if !leftover.is_empty() {
        return Err(format!(
            "CHANGELOG verification failed: fragments were not consumed and would \
             leak into the next release: {}. Empty or malformed fragments must be \
             fixed or removed, not left behind.",
            leftover.join(", ")
        ));
    }

    if cargo_lock_path.exists() {
        let lock = fs::read_to_string(cargo_lock_path).map_err(|e| {
            format!(
                "Cargo.lock verification failed: cannot read {}: {}",
                cargo_lock_path.display(),
                e
            )
        })?;
        let entry = Regex::new(&format!(
            r#"(?m)\[\[package\]\]\s*\nname\s*=\s*"{}"\s*\nversion\s*=\s*"([^"]+)""#,
            regex::escape(crate_name),
        ))
        .map_err(|e| format!("Failed to build Cargo.lock verification regex: {}", e))?;
        if let Some(caps) = entry.captures(&lock) {
            let locked_version = caps.get(1).map_or("", |m| m.as_str());
            if locked_version != new_version {
                return Err(format!(
                    "Cargo.lock verification failed: {} has `{}` at version `{}` but \
                     Cargo.toml was bumped to `{}` -- the lock file disagrees with the \
                     manifest, so the published crate would not match its lock entry",
                    cargo_lock_path.display(),
                    crate_name,
                    locked_version,
                    new_version
                ));
            }
        } else {
            println!(
                "Warning: {} has no [[package]] entry for `{}`; skipping lock agreement check",
                cargo_lock_path.display(),
                crate_name
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        classify_push_failure, collect_changelog_with_date, count_changelog_fragments,
        update_cargo_lock, verify_release_write, PushFailure,
    };
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("version-and-commit-{name}-{nanos}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn cargo_lock_update_scopes_to_named_package_entry() {
        let repo = temp_dir("lock-update");
        let cargo_lock = repo.join("Cargo.lock");
        fs::write(
            &cargo_lock,
            r#"[[package]]
name = "example-sum-package-name-helper"
version = "9.9.9"

[[package]]
name = "example-sum-package-name"
version = "0.13.0"
dependencies = [
 "clap",
]

[[package]]
name = "regex"
version = "1.12.3"
"#,
        )
        .unwrap();

        assert!(update_cargo_lock(&cargo_lock, "example-sum-package-name", "0.14.0").unwrap());

        let updated = fs::read_to_string(&cargo_lock).unwrap();
        assert!(updated.contains("name = \"example-sum-package-name\"\nversion = \"0.14.0\""));
        assert!(updated.contains("name = \"example-sum-package-name-helper\"\nversion = \"9.9.9\""));
        assert!(updated.contains("name = \"regex\"\nversion = \"1.12.3\""));
    }

    #[test]
    fn cargo_lock_update_is_idempotent_when_version_already_matches() {
        let repo = temp_dir("lock-idempotent");
        let cargo_lock = repo.join("Cargo.lock");
        let content = r#"[[package]]
name = "example-sum-package-name"
version = "0.14.0"
"#;
        fs::write(&cargo_lock, content).unwrap();

        assert!(!update_cargo_lock(&cargo_lock, "example-sum-package-name", "0.14.0").unwrap());
        assert_eq!(fs::read_to_string(&cargo_lock).unwrap(), content);
    }

    #[test]
    fn cargo_lock_update_returns_false_when_lock_file_is_absent() {
        let repo = temp_dir("lock-absent");
        let cargo_lock = repo.join("Cargo.lock");

        assert!(!update_cargo_lock(&cargo_lock, "example-sum-package-name", "0.14.0").unwrap());
        assert!(!cargo_lock.exists());
    }

    #[test]
    fn cargo_lock_update_returns_false_when_package_entry_is_absent() {
        let repo = temp_dir("lock-entry-absent");
        let cargo_lock = repo.join("Cargo.lock");
        let content = r#"[[package]]
name = "regex"
version = "1.12.3"
"#;
        fs::write(&cargo_lock, content).unwrap();

        assert!(!update_cargo_lock(&cargo_lock, "example-sum-package-name", "0.14.0").unwrap());
        assert_eq!(fs::read_to_string(&cargo_lock).unwrap(), content);
    }

    #[test]
    fn changelog_collection_removes_consumed_fragments_and_keeps_readme() {
        let repo = temp_dir("changelog-cleanup");
        let changelog_dir = repo.join("changelog.d");
        fs::create_dir_all(&changelog_dir).unwrap();

        let first_fragment = changelog_dir.join("20260608_fix_release_loop.md");
        let second_fragment = changelog_dir.join("20260608_note_release_loop.md");
        fs::write(
            &first_fragment,
            r#"---
bump: patch
---

### Fixed
- Prevent already-collected changelog fragments from triggering another release.
"#,
        )
        .unwrap();
        fs::write(
            &second_fragment,
            r#"### Changed
- Keep release commits from leaving stale changelog fragments behind.
"#,
        )
        .unwrap();
        fs::write(changelog_dir.join("README.md"), "Fragment instructions\n").unwrap();

        let changelog = repo.join("CHANGELOG.md");
        fs::write(
            &changelog,
            r#"# Changelog

## [0.1.0] - 2026-01-01

### Added
- Initial release
"#,
        )
        .unwrap();

        collect_changelog_with_date(
            changelog_dir.to_str().unwrap(),
            changelog.to_str().unwrap(),
            "0.2.0",
            "2026-06-08",
        );

        let updated = fs::read_to_string(&changelog).unwrap();
        assert!(updated.contains("## [0.2.0] - 2026-06-08"));
        assert!(updated.contains("Prevent already-collected changelog fragments"));
        assert!(updated.contains("Keep release commits from leaving stale changelog fragments"));
        assert!(!updated.contains("bump: patch"));
        assert!(!first_fragment.exists());
        assert!(!second_fragment.exists());
        assert!(changelog_dir.join("README.md").exists());
    }

    /// Regression test for issue #67: the Rust release job failed with
    /// "cannot rebase: Your index contains uncommitted changes." because the
    /// script staged the version bump (`git add`) and only afterwards ran
    /// `git rebase origin/<branch>`. `git rebase` refuses to run with a dirty
    /// index, so the release must rebase onto the remote BEFORE staging.
    ///
    /// This test reproduces both orderings against a real temporary repository
    /// and asserts the buggy order fails while the fixed order succeeds.
    #[test]
    fn rebase_must_run_before_staging_the_version_bump() {
        let repo = temp_dir("rebase-order");
        let git = |args: &[&str]| -> std::process::Output {
            Command::new("git")
                .args(args)
                .current_dir(&repo)
                .output()
                .expect("failed to run git")
        };

        // Some CI images default to `master`; force the initial branch to main.
        git(&["init", "-q", "."]);
        git(&["checkout", "-q", "-b", "main"]);
        git(&["config", "user.email", "ci@example.com"]);
        git(&["config", "user.name", "ci"]);

        // Commit A: the state the release job checks out.
        fs::write(repo.join("Cargo.toml"), "version = \"0.1.0\"\n").unwrap();
        git(&["add", "Cargo.toml"]);
        git(&["commit", "-qm", "A: initial"]);

        // Commit B on a parallel ref simulates origin/main advancing while the
        // release job was running (e.g. a concurrent release pushed to main).
        git(&["checkout", "-q", "-b", "upstream"]);
        fs::write(repo.join("remote.txt"), "remote change\n").unwrap();
        git(&["add", "remote.txt"]);
        git(&["commit", "-qm", "B: remote advanced"]);
        git(&["checkout", "-q", "main"]);

        // BUGGY ORDER: stage the bump first, then rebase -> git rejects the
        // dirty index. This is exactly what broke the release job.
        fs::write(repo.join("Cargo.toml"), "version = \"0.1.1\"\n").unwrap();
        git(&["add", "Cargo.toml"]);
        let buggy = git(&["rebase", "upstream"]);
        assert!(
            !buggy.status.success(),
            "rebase unexpectedly succeeded with a dirty index"
        );
        let buggy_err = String::from_utf8_lossy(&buggy.stderr);
        assert!(
            buggy_err.contains("cannot rebase") || buggy_err.contains("uncommitted changes"),
            "unexpected rebase error: {buggy_err}"
        );

        // Reset back to a clean tree at commit A.
        let _ = git(&["rebase", "--abort"]);
        git(&["reset", "-q", "--hard", "HEAD"]);

        // FIXED ORDER: rebase while the tree is clean, THEN stage and commit.
        let fixed = git(&["rebase", "upstream"]);
        assert!(
            fixed.status.success(),
            "rebase with a clean tree failed: {}",
            String::from_utf8_lossy(&fixed.stderr)
        );
        fs::write(repo.join("Cargo.toml"), "version = \"0.1.1\"\n").unwrap();
        git(&["add", "Cargo.toml"]);
        let commit = git(&["commit", "-qm", "chore: release 0.1.1"]);
        assert!(
            commit.status.success(),
            "commit after clean rebase failed: {}",
            String::from_utf8_lossy(&commit.stderr)
        );

        // The bump rides on top of the upstream commit, proving no work was lost.
        let log = git(&["log", "--oneline"]);
        let log_out = String::from_utf8_lossy(&log.stdout);
        assert!(log_out.contains("release 0.1.1"));
        assert!(log_out.contains("B: remote advanced"));
    }

    // --- issue #162: classify push failures before retrying -------------------

    #[test]
    fn repository_rules_are_classified_before_the_rebase_retry() {
        // A ruleset rejection also contains the word "rejected"; policy must
        // win over the retry heuristic.
        let ruleset = concat!(
            "Command `git` failed.\n",
            "stdout: To github.com:org/repo.git\n",
            "stderr: ! [remote rejected] main -> main (GH006: Protected branch update failed)\n",
            "error: GH013: Repository rule violations found for refs/heads/main.\n",
            "The push was rejected because changes must be made through a pull request.\n"
        );
        assert_eq!(
            classify_push_failure(ruleset),
            PushFailure::RepositoryRules,
            "a ruleset refusal must never be treated as a rebaseable race"
        );

        let lowercase_rules = "remote: error: push declined due to repository rule violations";
        assert_eq!(
            classify_push_failure(lowercase_rules),
            PushFailure::RepositoryRules
        );
    }

    #[test]
    fn a_lost_race_is_classified_for_the_rebase_retry() {
        let raced = concat!(
            "Command `git` failed.\n",
            "stderr: To github.com:org/repo.git\n",
            " ! [rejected]        main -> main (fetch first)\n",
            "error: failed to push some refs to 'github.com:org/repo.git'\n",
            "hint: Updates were rejected because the remote contains work that you do not have locally.\n"
        );
        assert_eq!(classify_push_failure(raced), PushFailure::LostRace);
    }

    #[test]
    fn an_unknown_push_failure_is_not_masked_as_a_race() {
        for real_error in [
            "ssh: connect to host github.com port 22: Connection timed out",
            "fatal: unable to access 'https://github.com/': Could not resolve host: github.com",
            "fatal: Authentication failed for 'https://github.com/org/repo.git/'",
        ] {
            assert_eq!(
                classify_push_failure(&format!("Command `git` failed.\nstderr: {real_error}")),
                PushFailure::Other,
                "{real_error} must surface as a real error, not trigger a rebase"
            );
        }
    }

    // --- issue #159: verify the release write before committing ---------------

    fn write_changelog(repo: &std::path::Path, body: &str) {
        fs::write(repo.join("CHANGELOG.md"), body).unwrap();
    }

    /// The verification paths must be anchored inside the temporary repo --
    /// relative paths would silently read the real repository's CHANGELOG.md.
    fn anchored(repo: &std::path::Path) -> (String, String, PathBuf) {
        (
            repo.join("CHANGELOG.md").to_string_lossy().to_string(),
            repo.join("changelog.d").to_string_lossy().to_string(),
            repo.join("Cargo.lock"),
        )
    }

    #[test]
    fn verification_accepts_a_consistent_release_write() {
        let repo = temp_dir("verify-ok");
        fs::create_dir_all(repo.join("changelog.d")).unwrap();
        write_changelog(
            &repo,
            "# Changelog\n\n## [0.2.0] - 2026-09-09\n\n### Fixed\n\n- something\n",
        );
        fs::write(
            repo.join("Cargo.lock"),
            "[[package]]\nname = \"crate\"\nversion = \"0.2.0\"\n",
        )
        .unwrap();
        let (changelog, dir, lock) = anchored(&repo);

        verify_release_write(&changelog, &dir, &lock, "crate", "0.2.0", true)
            .unwrap_or_else(|error| panic!("consistent write should verify: {error}"));
    }

    #[test]
    fn verification_requires_the_changelog_entry_only_when_fragments_were_collected() {
        let repo = temp_dir("verify-optional-entry");
        fs::create_dir_all(repo.join("changelog.d")).unwrap();
        // A version-only release (docs-only changes) legitimately collects no
        // fragments, so the changelog gains no entry.
        write_changelog(&repo, "# Changelog\n\n## [0.1.0] - 2026-01-01\n");
        let (changelog, dir, lock) = anchored(&repo);

        verify_release_write(&changelog, &dir, &lock, "crate", "0.2.0", false)
            .unwrap_or_else(|error| panic!("fragment-free release should verify: {error}"));

        // The same state with fragments collected must fail: the entry is missing.
        let error =
            verify_release_write(&changelog, &dir, &lock, "crate", "0.2.0", true).unwrap_err();
        assert!(
            error.contains("0.2.0"),
            "the failure must name the missing version entry, got: {error}"
        );
    }

    #[test]
    fn verification_fails_when_a_fragment_leaks_past_the_release() {
        let repo = temp_dir("verify-leftover");
        fs::create_dir_all(repo.join("changelog.d")).unwrap();
        write_changelog(&repo, "# Changelog\n\n## [0.2.0] - 2026-09-09\n");
        fs::write(
            repo.join("changelog.d/20260909_unconsumed.md"),
            "leftover\n",
        )
        .unwrap();
        let (changelog, dir, lock) = anchored(&repo);

        let error =
            verify_release_write(&changelog, &dir, &lock, "crate", "0.2.0", true).unwrap_err();
        assert!(
            error.contains("20260909_unconsumed.md"),
            "the failure must name the leaked fragment, got: {error}"
        );
    }

    #[test]
    fn collection_rejects_unreadable_fragments_before_mutation() {
        let repo = temp_dir("unreadable-fragment");
        let dir = repo.join("changelog.d");
        fs::create_dir_all(dir.join("broken.md")).unwrap();
        fs::write(dir.join("good.md"), "### Fixed\n- Keep this fragment.\n").unwrap();
        let changelog = repo.join("CHANGELOG.md");
        fs::write(&changelog, "# Changelog\n").unwrap();
        let result = std::panic::catch_unwind(|| {
            collect_changelog_with_date(
                dir.to_str().unwrap(),
                changelog.to_str().unwrap(),
                "1.0.0",
                "2026-10-10",
            );
        });
        assert!(result.is_err(), "unreadable fragment must be fatal");
        assert_eq!(fs::read_to_string(changelog).unwrap(), "# Changelog\n");
        assert!(dir.join("good.md").is_file());
        fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn counting_and_verification_reject_fragment_directory_read_errors() {
        let repo = temp_dir("unreadable-directory");
        fs::write(repo.join("changelog.d"), "not a directory").unwrap();
        let (changelog, dir, lock) = anchored(&repo);
        assert!(std::panic::catch_unwind(|| count_changelog_fragments(&dir)).is_err());
        let error =
            verify_release_write(&changelog, &dir, &lock, "crate", "1.0.0", false).unwrap_err();
        assert!(error.contains("changelog.d"));
        fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn verification_fails_when_the_lock_disagrees_with_the_manifest() {
        let repo = temp_dir("verify-lock");
        fs::create_dir_all(repo.join("changelog.d")).unwrap();
        write_changelog(&repo, "# Changelog\n\n## [0.2.0] - 2026-09-09\n");
        fs::write(
            repo.join("Cargo.lock"),
            "[[package]]\nname = \"crate\"\nversion = \"0.1.9\"\n",
        )
        .unwrap();
        let (changelog, dir, lock) = anchored(&repo);

        let error =
            verify_release_write(&changelog, &dir, &lock, "crate", "0.2.0", true).unwrap_err();
        assert!(
            error.contains("Cargo.lock verification failed") && error.contains("0.1.9"),
            "the failure must show the disagreement, got: {error}"
        );
    }

    #[test]
    fn fragment_counting_ignores_the_readme() {
        let repo = temp_dir("count-fragments");
        let dir = repo.join("changelog.d");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("README.md"), "explanatory\n").unwrap();
        assert_eq!(count_changelog_fragments(dir.to_str().unwrap()), 0);
        fs::write(dir.join("20260909_a.md"), "a\n").unwrap();
        fs::write(dir.join("20260909_b.md"), "b\n").unwrap();
        assert_eq!(count_changelog_fragments(dir.to_str().unwrap()), 2);
    }
}

#[cfg(not(test))]
fn main() -> std::io::Result<()> {
    let bump_type = match get_arg("bump-type") {
        Some(bt) => bt,
        None => {
            eprintln!("Usage: rust-script scripts/version-and-commit.rs --bump-type <major|minor|patch> [--description <desc>] [--rust-root <path>] [--tag-prefix <prefix>] [--release-label <label>]");
            exit(1);
        }
    };

    if !["major", "minor", "patch"].contains(&bump_type.as_str()) {
        eprintln!(
            "Invalid bump type: {}. Must be major, minor, or patch.",
            bump_type
        );
        exit(1);
    }

    let rust_root = match rust_paths::get_rust_root(None, true) {
        Ok(root) => root,
        Err(e) => {
            eprintln!("Error: {}", e);
            exit(1);
        }
    };
    let description = get_arg("description");
    let tag_prefix = get_arg("tag-prefix")
        .unwrap_or_else(|| release_naming::tag_prefix_for_rust_root(&rust_root).to_string());
    let release_label = get_arg("release-label");
    let cargo_toml = rust_paths::get_cargo_toml_path(&rust_root);
    let package_manifest = match rust_paths::get_package_manifest_path(&cargo_toml) {
        Ok(path) => path,
        Err(e) => {
            eprintln!("Error: {}", e);
            exit(1);
        }
    };
    let changelog_dir = get_changelog_dir(&rust_root);
    let changelog_file = get_changelog_path(&rust_root);

    // Inspect all repository changes before any release write or remote request.
    release_index::ReleaseIndex::new(Path::new(&rust_root), &package_manifest)
        .and_then(|index| index.validate())
        .unwrap_or_else(|e| {
            eprintln!("::error::{e}");
            exit(1);
        });

    // Configure git
    let _ = exec("git", &["config", "user.name", "github-actions[bot]"]);
    let _ = exec(
        "git",
        &[
            "config",
            "user.email",
            "github-actions[bot]@users.noreply.github.com",
        ],
    );

    // Sync with the latest remote state BEFORE touching any files.
    //
    // This must happen while the working tree is clean: `git rebase` refuses to
    // run with a dirty index ("cannot rebase: Your index contains uncommitted
    // changes"). Rebasing after staging the version bump is what previously
    // broke the release job. Rebasing first also means the version bump is
    // computed from the most recent state of the branch (matches the JS
    // version-and-commit.mjs ordering).
    let current_branch =
        exec("git", &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_else(|_| "main".to_string());
    if let Err(e) = exec("git", &["fetch", "origin", &current_branch]) {
        eprintln!("Warning: Could not fetch origin/{}: {}", current_branch, e);
    } else {
        // Count the commits origin/<branch> has that HEAD does not. Being merely
        // ahead of the remote is not being behind, and needs no rebase.
        let behind: u32 = exec(
            "git",
            &[
                "rev-list",
                "--count",
                &format!("HEAD..origin/{}", current_branch),
            ],
        )
        .ok()
        .and_then(|out| out.trim().parse().ok())
        .unwrap_or(0);
        if behind > 0 {
            println!(
                "Local branch is behind origin/{} by {} commit(s), rebasing...",
                current_branch, behind
            );
            if let Err(e) = exec("git", &["rebase", &format!("origin/{}", current_branch)]) {
                eprintln!("Error rebasing onto origin/{}: {}", current_branch, e);
                let _ = exec("git", &["rebase", "--abort"]);
                exit(1);
            }
        }
    }

    // Recompute the allowlist after synchronization, including new fragments.
    let release_index = release_index::ReleaseIndex::new(Path::new(&rust_root), &package_manifest)
        .unwrap_or_else(|e| {
            eprintln!("::error::{e}");
            exit(1);
        });

    // Get current version
    let content = match fs::read_to_string(&package_manifest) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading {}: {}", package_manifest.display(), e);
            exit(1);
        }
    };

    let current = match Version::parse(&content) {
        Some(v) => v,
        None => {
            eprintln!(
                "Error: Could not parse version from {}",
                package_manifest.display()
            );
            exit(1);
        }
    };

    let initial_bump = current.bump(&bump_type);

    let crate_name = match get_crate_name(package_manifest.to_string_lossy().as_ref()) {
        Ok(name) => name,
        Err(e) => {
            eprintln!("Error: {}", e);
            exit(1);
        }
    };

    let max_published = get_max_published_version(&crate_name);
    if let Some((ma, mi, pa)) = max_published {
        println!("Max published version on crates.io: {}.{}.{}", ma, mi, pa);
    } else {
        println!("No versions published on crates.io yet (or crate not found)");
    }

    println!(
        "Initial bump ({}) from {}.{}.{}: {}",
        bump_type, current.major, current.minor, current.patch, initial_bump
    );

    let new_version =
        ensure_version_exceeds_published(&initial_bump, &crate_name, &tag_prefix, max_published);

    if new_version != initial_bump {
        println!(
            "Adjusted version from {} to {} to exceed published versions",
            initial_bump, new_version
        );
    }

    println!("Final release version: {}", new_version);

    // Update version in Cargo.toml
    if let Err(e) = update_cargo_toml(package_manifest.to_string_lossy().as_ref(), &new_version) {
        eprintln!("Error: {}", e);
        exit(1);
    }

    let cargo_lock_path = rust_paths::get_cargo_lock_path(&rust_root);
    let lock_updated = match update_cargo_lock(&cargo_lock_path, &crate_name, &new_version) {
        Ok(updated) => updated,
        Err(e) => {
            eprintln!("Error updating Cargo.lock: {}", e);
            exit(1);
        }
    };

    // Collect changelog fragments
    let fragments_before = count_changelog_fragments(&changelog_dir);
    collect_changelog(&changelog_dir, &changelog_file, &new_version);

    // The release commit is never re-checked by CI, so the write is verified
    // before it is staged (issue #159).
    if let Err(e) = verify_release_write(
        &changelog_file,
        &changelog_dir,
        &cargo_lock_path,
        &crate_name,
        &new_version,
        fragments_before > 0,
    ) {
        eprintln!("::error title=Release write verification failed::{}", e);
        exit(1);
    }

    release_index.validate().unwrap_or_else(|e| {
        eprintln!("::error::{e}");
        exit(1);
    });

    // Stage Cargo.toml, Cargo.lock if changed, CHANGELOG.md, and consumed changelog fragments.
    let package_manifest_str = package_manifest.to_string_lossy().to_string();
    let cargo_lock_str = cargo_lock_path.to_string_lossy().to_string();
    let mut add_args = vec!["add", &package_manifest_str, &changelog_file];
    if lock_updated {
        add_args.push(&cargo_lock_str);
    }
    if let Err(e) = exec("git", &add_args) {
        eprintln!("Error staging release files: {}", e);
        exit(1);
    }
    if Path::new(&changelog_dir).exists() {
        if let Err(e) = exec("git", &["add", "-A", &changelog_dir]) {
            eprintln!("Error staging changelog fragments: {}", e);
            exit(1);
        }
    }

    // Validate the complete index again, including anything staged beforehand.
    release_index.validate().unwrap_or_else(|e| {
        eprintln!("::error::{e}");
        exit(1);
    });

    // Check if there are changes to commit
    if exec_check("git", &["diff", "--cached", "--quiet"]) {
        println!("No changes to commit");
        set_output("version_committed", "false")?;
        set_output("new_version", &new_version)?;
        return Ok(());
    }

    // Commit changes
    let label_suffix = release_label
        .as_ref()
        .map(|l| format!(" ({})", l))
        .unwrap_or_default();
    let commit_msg = match &description {
        Some(desc) => format!(
            "chore: release {}{}{}\n\n{}",
            tag_prefix, new_version, label_suffix, desc
        ),
        None => format!(
            "chore: release {}{}{}",
            tag_prefix, new_version, label_suffix
        ),
    };

    if let Err(e) = exec("git", &["commit", "-m", &commit_msg]) {
        eprintln!("Error committing: {}", e);
        exit(1);
    }
    println!("Committed version {}", new_version);

    // Push changes with retry -- but only a genuine race is retried (issue
    // #162): a lost non-fast-forward is fixed by rebasing onto the new remote
    // tip, while a repository-ruleset refusal or any other failure is reported
    // as what it is instead of being masked as a merge conflict.
    let max_push_attempts = 3;
    for attempt in 1..=max_push_attempts {
        match exec("git", &["push"]) {
            Ok(_) => break,
            Err(push_error) => match classify_push_failure(&push_error) {
                PushFailure::RepositoryRules => {
                    eprintln!(
                        "::error title=Push declined by repository rules::The push to branch '{}' was declined by a repository rule (a GH006/GH013-class rejection, e.g. 'changes must be made through a pull request' or a protected-branch ruleset). Rebasing and retrying cannot change repository policy: release this change through a pull request, or adjust the ruleset so the release bot may push.\n--- git output ---\n{}",
                        current_branch, push_error
                    );
                    exit(1);
                }
                PushFailure::Other => {
                    eprintln!("Error pushing: {}", push_error);
                    exit(1);
                }
                PushFailure::LostRace => {
                    if attempt < max_push_attempts {
                        eprintln!(
                            "Push rejected as non-fast-forward (attempt {}/{}); the remote branch moved. Pulling with rebase and retrying...",
                            attempt, max_push_attempts
                        );
                        if let Err(rebase_err) =
                            exec("git", &["pull", "--rebase", "origin", &current_branch])
                        {
                            eprintln!("Error during pull --rebase: {}", rebase_err);
                            let _ = exec("git", &["rebase", "--abort"]);
                            exit(1);
                        }
                    } else {
                        eprintln!(
                            "Error pushing after {} attempts: {}",
                            max_push_attempts, push_error
                        );
                        exit(1);
                    }
                }
            },
        }
    }

    // Create tag only after the release commit is on the remote, so that a
    // `pull --rebase` retry above can never leave the tag on an orphaned
    // pre-rebase commit (see issue #94).
    let tag_name = format!("{}{}", tag_prefix, new_version);
    let tag_msg = match &description {
        Some(desc) => format!("Release {}{}\n\n{}", tag_name, label_suffix, desc),
        None => format!("Release {}{}", tag_name, label_suffix),
    };

    if let Err(e) = exec("git", &["tag", "-a", &tag_name, "-m", &tag_msg]) {
        eprintln!("Error creating tag: {}", e);
        exit(1);
    }
    println!("Created tag {}", tag_name);

    // Push exactly the release tag: `push --tags` would publish every local
    // tag, including unrelated ones left behind by other jobs or retries.
    if let Err(e) = exec("git", &["push", "origin", &tag_name]) {
        eprintln!("Error pushing tag {}: {}", tag_name, e);
        exit(1);
    }
    println!("Pushed changes and tag {}", tag_name);

    set_output("version_committed", "true")?;
    set_output("new_version", &new_version)?;
    Ok(())
}
