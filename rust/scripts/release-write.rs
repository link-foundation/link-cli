//! Release-write helpers shared by `version-and-commit.rs` (issue #134):
//! keeping Cargo.lock in step with the bump, verifying the write before it is
//! committed, and classifying a failed `git push`.
//!
//! Included with `#[path = "release-write.rs"] mod release_write;`;
//! its tests run through `rust-script --test rust/scripts/version-and-commit.rs`.

#![allow(dead_code)]

use crate::release_changelog::list_fragments;
use regex::Regex;
use std::fs;
use std::path::Path;

pub fn cargo_lock_entry_regex(crate_name: &str) -> Regex {
    Regex::new(&format!(
        r#"(?m)(\[\[package\]\]\s*\nname\s*=\s*"{}"\s*\nversion\s*=\s*")([^"]+)(")"#,
        regex::escape(crate_name),
    ))
    .expect("valid Cargo.lock regex")
}

/// Keep the crate's own `[[package]]` entry in Cargo.lock in step with the
/// bumped manifest; `cargo publish` would otherwise rewrite it in a dirty tree.
/// Returns whether the lock file changed.
pub fn update_cargo_lock(
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

    let content = fs::read_to_string(cargo_lock_path)
        .map_err(|e| format!("Failed to read {}: {}", cargo_lock_path.display(), e))?;
    let re = cargo_lock_entry_regex(crate_name);
    if !re.is_match(&content) {
        println!(
            "Warning: Could not find [[package]] entry for `{}` in {} (lock file left untouched)",
            crate_name,
            cargo_lock_path.display()
        );
        return Ok(false);
    }

    let new_content = re.replace(&content, format!("${{1}}{}${{3}}", new_version).as_str());
    if new_content == content {
        println!("Cargo.lock already at version {}", new_version);
        return Ok(false);
    }

    fs::write(cargo_lock_path, new_content.as_ref())
        .map_err(|e| format!("Failed to write {}: {}", cargo_lock_path.display(), e))?;
    println!(
        "Updated {} to version {}",
        cargo_lock_path.display(),
        new_version
    );
    Ok(true)
}

/// The version commit is pushed straight to main with GITHUB_TOKEN, which does
/// not trigger any workflow, so nothing ever re-checks it: verify the write
/// itself before it is staged.
pub fn verify_release_write(
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
        .expect("valid changelog regex");
        if !entry.is_match(&changelog) {
            return Err(format!(
                "CHANGELOG verification failed: {} has no '## [{}] - YYYY-MM-DD' entry \
                 after collecting the fragments",
                changelog_file, new_version
            ));
        }
    }

    let leftover: Vec<String> = list_fragments(Path::new(changelog_dir))?
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    if !leftover.is_empty() {
        return Err(format!(
            "CHANGELOG verification failed: fragments were not consumed and would be \
             released again: {}",
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
        if let Some(caps) = cargo_lock_entry_regex(crate_name).captures(&lock) {
            let locked = caps.get(2).map_or("", |m| m.as_str());
            if locked != new_version {
                return Err(format!(
                    "Cargo.lock verification failed: {} has `{}` at version `{}` but \
                     Cargo.toml was bumped to `{}`",
                    cargo_lock_path.display(),
                    crate_name,
                    locked,
                    new_version
                ));
            }
        }
    }

    Ok(())
}

/// Not every failed `git push` is a lost race. A repository ruleset rejection
/// arrives shaped like a non-fast-forward, and rebasing against a policy
/// refusal only burns the job's budget before dying with a misleading error,
/// so the failure is classified first (same patterns as the
/// rust-ai-driven-development-pipeline-template).
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
pub enum PushFailure {
    /// A repository ruleset refuses the push; retrying cannot fix policy.
    RepositoryRules,
    /// The remote branch moved; rebase onto it and try again.
    LostRace,
    /// Anything else: report the real error instead of masking it.
    Other,
}

pub fn classify_push_failure(raw_output: &str) -> PushFailure {
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
