#!/usr/bin/env rust-script
//! Bump version in Cargo.toml and commit changes
//! Used by the CI/CD pipeline for releases
//!
//! IMPORTANT: This script checks crates.io (the source of truth for Rust packages),
//! NOT git tags. This is critical because:
//! - Git tags can exist without the package being published
//! - GitHub releases create tags but don't publish to crates.io
//! - Only crates.io publication means users can actually install the package
//!
//! Supports both single-language and multi-language repository structures:
//! - Single-language: Cargo.toml and changelog.d/ in repository root
//! - Multi-language: Cargo.toml and changelog.d/ in rust/ subfolder
//!
//! The release write is verified before it is committed (issue #134): the
//! version bump, the CHANGELOG.md entry, the consumed changelog fragments and
//! the Cargo.lock entry must all land in one commit. Previously a missing
//! `rust/CHANGELOG.md` made `git add` fail silently, nothing was staged, the
//! script printed "No changes to commit" and the workflow still published
//! the uncommitted version, so every push to main re-released the same
//! fragments as a new patch version.
//!
//! Set `VERSION_AND_COMMIT_DEBUG=1` to print every git command and its output.
//!
//! Usage: rust-script rust/scripts/version-and-commit.rs --bump-type <major|minor|patch> [--description <desc>] [--rust-root <path>] [--tag-prefix <prefix>] [--release-label <label>]
//!
//! Tests: rust-script --test rust/scripts/version-and-commit.rs
//!
//! ```cargo
//! [dependencies]
//! regex = "1"
//! chrono = "0.4"
//! ureq = "2"
//! serde = { version = "1", features = ["derive"] }
//! serde_json = "1"
//! ```

// The test harness replaces main, so release-only helpers look unused there.
#![cfg_attr(test, allow(dead_code))]

use chrono::Utc;
use regex::Regex;
use serde::Deserialize;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{exit, Command};

#[path = "rust-paths.rs"]
mod rust_paths;

#[path = "release-changelog.rs"]
mod release_changelog;

#[path = "release-write.rs"]
mod release_write;

use release_changelog::collect_changelog_with_date;
use release_write::{classify_push_failure, update_cargo_lock, verify_release_write, PushFailure};

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

fn set_output(key: &str, value: &str) {
    if let Ok(output_file) = env::var("GITHUB_OUTPUT") {
        if let Err(e) = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&output_file)
            .and_then(|mut f| writeln!(f, "{}={}", key, value))
        {
            eprintln!("Warning: Could not write to GITHUB_OUTPUT: {}", e);
        }
    }
    println!("Output: {}={}", key, value);
}

fn debug_enabled() -> bool {
    env::var("VERSION_AND_COMMIT_DEBUG").map_or(false, |v| !v.is_empty() && v != "0")
}

/// Run a command (in `dir` when given). Failures carry stdout and stderr,
/// because git prints push rejections on either stream.
fn exec_in(dir: Option<&Path>, command: &str, args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new(command);
    cmd.args(args);
    if let Some(dir) = dir {
        cmd.current_dir(dir);
    }
    let result = match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if output.status.success() {
                Ok(stdout)
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(format!("Command failed: {} {}", stderr.trim(), stdout)
                    .trim()
                    .to_string())
            }
        }
        Err(e) => Err(format!("Failed to execute: {}", e)),
    };
    if debug_enabled() {
        eprintln!("[debug] {} {} -> {:?}", command, args.join(" "), result);
    }
    result
}

fn exec(command: &str, args: &[&str]) -> Result<String, String> {
    exec_in(None, command, args)
}

fn exec_check(command: &str, args: &[&str]) -> bool {
    Command::new(command)
        .args(args)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
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

#[derive(Deserialize)]
struct CratesIoCrate {
    versions: Option<Vec<CratesIoVersionEntry>>,
}

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

/// Stage the release write. Every `git add` failure is fatal: ignoring one is
/// what used to turn a release into "No changes to commit" (issue #134).
fn stage_release_files(
    repo: Option<&Path>,
    package_manifest: &str,
    changelog_file: &str,
    changelog_dir: &str,
    cargo_lock: Option<&str>,
) -> Result<Vec<String>, String> {
    let mut paths = vec![package_manifest];
    if repo.map_or(Path::new(changelog_file).exists(), |dir| {
        dir.join(changelog_file).exists()
    }) {
        paths.push(changelog_file);
    }
    if let Some(lock) = cargo_lock {
        paths.push(lock);
    }
    let mut add_args = vec!["add", "--"];
    add_args.extend(paths);
    exec_in(repo, "git", &add_args).map_err(|e| format!("Error staging release files: {}", e))?;

    // `-A` also stages the deletion of every consumed fragment.
    exec_in(repo, "git", &["add", "-A", "--", changelog_dir])
        .map_err(|e| format!("Error staging consumed changelog fragments: {}", e))?;

    let staged = exec_in(repo, "git", &["diff", "--cached", "--name-only"])
        .map_err(|e| format!("Error listing staged files: {}", e))?;
    Ok(staged
        .lines()
        .map(str::to_string)
        .filter(|s| !s.is_empty())
        .collect())
}

fn fail(message: &str) -> ! {
    eprintln!("::error title=Rust release failed::{}", message);
    exit(1);
}

fn main() {
    let bump_type = match get_arg("bump-type") {
        Some(bt) => bt,
        None => {
            eprintln!("Usage: rust-script rust/scripts/version-and-commit.rs --bump-type <major|minor|patch> [--description <desc>] [--rust-root <path>] [--tag-prefix <prefix>] [--release-label <label>]");
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

    let description = get_arg("description");
    let tag_prefix = get_arg("tag-prefix").unwrap_or_else(|| "v".to_string());
    let release_label = get_arg("release-label");
    let rust_root = match rust_paths::get_rust_root(None, true) {
        Ok(root) => root,
        Err(e) => fail(&e),
    };
    let cargo_toml = rust_paths::get_cargo_toml_path(&rust_root);
    let package_manifest = match rust_paths::get_package_manifest_path(&cargo_toml) {
        Ok(path) => path,
        Err(e) => fail(&e),
    };
    let package_manifest_str = package_manifest.to_string_lossy().to_string();
    let changelog_dir = get_changelog_dir(&rust_root);
    let changelog_file = get_changelog_path(&rust_root);

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

    // Sync with the remote BEFORE touching any file: `git rebase` refuses to
    // run with a dirty index, and the bump must start from the newest state.
    let current_branch =
        exec("git", &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_else(|_| "main".to_string());
    if let Err(e) = exec("git", &["fetch", "origin", &current_branch]) {
        eprintln!("Warning: Could not fetch origin/{}: {}", current_branch, e);
    } else {
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
                let _ = exec("git", &["rebase", "--abort"]);
                fail(&format!(
                    "Error rebasing onto origin/{}: {}",
                    current_branch, e
                ));
            }
        }
    }

    // Get current version
    let content = match fs::read_to_string(&package_manifest) {
        Ok(c) => c,
        Err(e) => fail(&format!(
            "Error reading {}: {}",
            package_manifest.display(),
            e
        )),
    };

    let current = match Version::parse(&content) {
        Some(v) => v,
        None => fail(&format!(
            "Could not parse version from {}",
            package_manifest.display()
        )),
    };

    let initial_bump = current.bump(&bump_type);

    let crate_name = match get_crate_name(&package_manifest_str) {
        Ok(name) => name,
        Err(e) => fail(&e),
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

    if let Err(e) = update_cargo_toml(&package_manifest_str, &new_version) {
        fail(&e);
    }

    let cargo_lock_path = rust_paths::get_cargo_lock_path(&rust_root);
    let lock_updated = match update_cargo_lock(&cargo_lock_path, &crate_name, &new_version) {
        Ok(updated) => updated,
        Err(e) => fail(&format!("Error updating Cargo.lock: {}", e)),
    };

    let date_str = Utc::now().format("%Y-%m-%d").to_string();
    let collected =
        match collect_changelog_with_date(&changelog_dir, &changelog_file, &new_version, &date_str)
        {
            Ok(count) => count,
            Err(e) => fail(&e),
        };

    if let Err(e) = verify_release_write(
        &changelog_file,
        &changelog_dir,
        &cargo_lock_path,
        &crate_name,
        &new_version,
        collected > 0,
    ) {
        fail(&e);
    }

    let cargo_lock_str = cargo_lock_path.to_string_lossy().to_string();
    let staged = match stage_release_files(
        None,
        &package_manifest_str,
        &changelog_file,
        &changelog_dir,
        lock_updated.then_some(cargo_lock_str.as_str()),
    ) {
        Ok(staged) => staged,
        Err(e) => fail(&e),
    };
    println!("Staged release files: {}", staged.join(", "));

    // The bump always changes Cargo.toml, so an empty index means the release
    // write was lost; publishing now would release a version that main never
    // records (the 1.0.6-1.0.8 drift of issue #134).
    if staged.is_empty() {
        fail(&format!(
            "Nothing was staged for release {}{}; refusing to publish a version that is not committed",
            tag_prefix, new_version
        ));
    }

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
        fail(&format!("Error committing: {}", e));
    }
    println!("Committed version {}", new_version);

    // Only a genuine race is retried: a lost non-fast-forward is fixed by
    // rebasing onto the new remote tip, while a repository-ruleset refusal or
    // any other failure is reported as what it is.
    let max_push_attempts = 3;
    for attempt in 1..=max_push_attempts {
        let push_error = match exec(
            "git",
            &["push", "origin", &format!("HEAD:{}", current_branch)],
        ) {
            Ok(_) => break,
            Err(e) => e,
        };
        match classify_push_failure(&push_error) {
            PushFailure::RepositoryRules => fail(&format!(
                "The push to '{}' was declined by a repository rule (GH006/GH013). Rebasing cannot change repository policy: allow the release bot to push to '{}' or release through a pull request. git output: {}",
                current_branch, current_branch, push_error
            )),
            PushFailure::Other => fail(&format!("Error pushing: {}", push_error)),
            PushFailure::LostRace if attempt < max_push_attempts => {
                eprintln!(
                    "Push rejected as non-fast-forward (attempt {}/{}); pulling with rebase and retrying...",
                    attempt, max_push_attempts
                );
                if let Err(rebase_err) =
                    exec("git", &["pull", "--rebase", "origin", &current_branch])
                {
                    let _ = exec("git", &["rebase", "--abort"]);
                    fail(&format!("Error during pull --rebase: {}", rebase_err));
                }
            }
            PushFailure::LostRace => fail(&format!(
                "Error pushing after {} attempts: {}",
                max_push_attempts, push_error
            )),
        }
    }

    // Tag only after the commit is on the remote, so a rebase retry can never
    // leave the tag on an orphaned pre-rebase commit; then push exactly this
    // tag (`push --tags` would publish every local tag).
    let tag_name = format!("{}{}", tag_prefix, new_version);
    let tag_msg = match &description {
        Some(desc) => format!("Release {}{}\n\n{}", tag_name, label_suffix, desc),
        None => format!("Release {}{}", tag_name, label_suffix),
    };

    if let Err(e) = exec("git", &["tag", "-a", &tag_name, "-m", &tag_msg]) {
        fail(&format!("Error creating tag: {}", e));
    }
    println!("Created tag {}", tag_name);

    if let Err(e) = exec("git", &["push", "origin", &tag_name]) {
        fail(&format!("Error pushing tag {}: {}", tag_name, e));
    }
    println!("Pushed release commit and tag {}", tag_name);

    set_output("version_committed", "true");
    set_output("new_version", &new_version);
}

#[cfg(test)]
mod tests {
    use super::*;
    use release_changelog::insert_changelog_entry;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = env::temp_dir().join(format!("version-and-commit-{name}-{nanos}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn write_fragment(dir: &Path, name: &str, body: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join(name), format!("---\nbump: patch\n---\n\n{body}\n")).unwrap();
    }

    fn git(repo: &Path, args: &[&str]) -> String {
        exec_in(Some(repo), "git", args).unwrap_or_else(|e| panic!("git {args:?}: {e}"))
    }

    #[test]
    fn cargo_lock_entry_of_the_crate_is_bumped_and_others_are_kept() {
        let root = temp_dir("cargo-lock");
        let lock = root.join("Cargo.lock");
        fs::write(
            &lock,
            "[[package]]\nname = \"link-cli\"\nversion = \"0.1.0\"\n\n[[package]]\nname = \"link-cli-helper\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        assert!(update_cargo_lock(&lock, "link-cli", "1.0.9").unwrap());
        let content = fs::read_to_string(&lock).unwrap();
        assert!(content.contains("name = \"link-cli\"\nversion = \"1.0.9\""));
        assert!(content.contains("name = \"link-cli-helper\"\nversion = \"0.1.0\""));
        assert!(!update_cargo_lock(&lock, "link-cli", "1.0.9").unwrap());
    }

    #[test]
    fn verification_rejects_leftover_fragments_and_lock_mismatch() {
        let root = temp_dir("verify");
        let dir = root.join("changelog.d");
        let changelog = root.join("CHANGELOG.md");
        let lock = root.join("Cargo.lock");
        fs::write(
            &changelog,
            insert_changelog_entry(None, "\n## [1.0.9] - 2026-10-24\n\nX.\n"),
        )
        .unwrap();
        fs::write(
            &lock,
            "[[package]]\nname = \"link-cli\"\nversion = \"1.0.9\"\n",
        )
        .unwrap();
        let verify = |expect_entry: bool, version: &str| {
            verify_release_write(
                changelog.to_str().unwrap(),
                dir.to_str().unwrap(),
                &lock,
                "link-cli",
                version,
                expect_entry,
            )
        };

        assert!(verify(true, "1.0.9").is_ok());
        assert!(verify(true, "1.0.10").unwrap_err().contains("CHANGELOG"));
        assert!(verify(false, "1.0.10").unwrap_err().contains("Cargo.lock"));
        write_fragment(&dir, "20260101_000000_left.md", "Left behind.");
        assert!(verify(true, "1.0.9").unwrap_err().contains("not consumed"));
    }

    #[test]
    fn staging_records_bump_changelog_lock_and_fragment_removal() {
        // Before the fix, `git add rust/Cargo.toml rust/CHANGELOG.md` failed on
        // the missing CHANGELOG.md, the error was ignored, and the release was
        // published with nothing committed.
        let repo = temp_dir("staging");
        git(&repo, &["init", "--quiet", "--initial-branch=main"]);
        git(&repo, &["config", "user.name", "test"]);
        git(&repo, &["config", "user.email", "test@example.com"]);
        let rust = repo.join("rust");
        fs::create_dir_all(&rust).unwrap();
        fs::write(
            rust.join("Cargo.toml"),
            "[package]\nname = \"link-cli\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::write(
            rust.join("Cargo.lock"),
            "[[package]]\nname = \"link-cli\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        write_fragment(
            &rust.join("changelog.d"),
            "20260101_000000_fix.md",
            "A fix.",
        );
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "--quiet", "-m", "initial"]);

        update_cargo_toml(rust.join("Cargo.toml").to_str().unwrap(), "1.0.9").unwrap();
        assert!(update_cargo_lock(&rust.join("Cargo.lock"), "link-cli", "1.0.9").unwrap());
        collect_changelog_with_date(
            rust.join("changelog.d").to_str().unwrap(),
            rust.join("CHANGELOG.md").to_str().unwrap(),
            "1.0.9",
            "2026-10-24",
        )
        .unwrap();

        let staged = stage_release_files(
            Some(&repo),
            "rust/Cargo.toml",
            "rust/CHANGELOG.md",
            "rust/changelog.d",
            Some("rust/Cargo.lock"),
        )
        .unwrap();

        assert_eq!(
            staged,
            vec![
                "rust/CHANGELOG.md",
                "rust/Cargo.lock",
                "rust/Cargo.toml",
                "rust/changelog.d/20260101_000000_fix.md",
            ]
        );
        let status = git(&repo, &["status", "--porcelain", "--untracked-files=all"]);
        assert!(
            !status.contains("??"),
            "unstaged release files left:\n{status}"
        );
    }

    #[test]
    fn staging_errors_are_reported_instead_of_ignored() {
        let repo = temp_dir("staging-error");
        git(&repo, &["init", "--quiet", "--initial-branch=main"]);
        let error = stage_release_files(
            Some(&repo),
            "rust/Cargo.toml",
            "rust/CHANGELOG.md",
            "rust/changelog.d",
            None,
        )
        .unwrap_err();
        assert!(error.contains("Error staging release files"), "{error}");
    }

    #[test]
    fn repository_rules_are_classified_before_the_rebase_retry() {
        let output = "remote: error: GH013: Repository rule violations found for refs/heads/main.\n ! [remote rejected] HEAD -> main (push declined due to repository rule violations)";
        assert_eq!(classify_push_failure(output), PushFailure::RepositoryRules);
    }

    #[test]
    fn a_lost_race_is_classified_for_the_rebase_retry() {
        let output = " ! [rejected]        HEAD -> main (fetch first)\nerror: failed to push some refs\nhint: Updates were rejected because the remote contains work that you do not have locally.";
        assert_eq!(classify_push_failure(output), PushFailure::LostRace);
        assert_eq!(
            classify_push_failure("fatal: could not read Username"),
            PushFailure::Other
        );
    }
}
