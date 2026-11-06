#!/usr/bin/env rust-script
//! Detect code changes for CI/CD pipeline
//!
//! This script detects what types of files have changed in the latest commit
//! and outputs the results for use in GitHub Actions workflow conditions.
//!
//! Key behavior:
//! - For PRs: detects GitHub Actions' synthetic merge commit and uses
//!   HEAD^2^..HEAD^2 to get the per-commit diff of the actual PR head,
//!   so a commit touching only non-code files correctly skips CI jobs
//!   even when earlier commits in the same PR touched code files.
//! - For pushes: compares HEAD against HEAD^
//! - Excludes certain folders and file types from "code changes" detection
//!
//! Excluded from code changes (don't require changelog fragments):
//! - Markdown files (*.md) in any folder
//! - changelog.d/ folder (changelog fragments)
//! - docs/ folder (documentation)
//! - experiments/ folder (experimental scripts)
//! - examples/ folder (example scripts)
//!
//! Usage: rust-script rust/scripts/detect-code-changes.rs
//!
//! Environment variables (set by GitHub Actions):
//!   - GITHUB_EVENT_NAME: 'pull_request' or 'push'
//!
//! Outputs (written to GITHUB_OUTPUT):
//!   - rs-changed: 'true' if any .rs files changed
//!   - toml-changed: 'true' if any .toml or Cargo.lock files changed
//!   - mjs-changed: 'true' if any .mjs files changed
//!   - docs-changed: 'true' if any .md files changed
//!   - workflow-changed: 'true' if any .github/workflows/ files changed
//!   - any-code-changed: 'true' if any code files changed (excludes docs, changelog.d, experiments, examples)
//!
//! ```cargo
//! [dependencies]
//! regex = "1"
//! ```

use regex::Regex;
use std::env;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;

type DetectionResult<T> = Result<T, Box<dyn std::error::Error>>;

fn git(repo: &Path, args: &[&str]) -> DetectionResult<String> {
    let output = Command::new("git").args(args).current_dir(repo).output()?;
    if !output.status.success() {
        return Err(format!(
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

fn set_output(name: &str, value: &str) -> std::io::Result<()> {
    if let Ok(output_file) = env::var("GITHUB_OUTPUT") {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&output_file)?;
        writeln!(file, "{}={}", name, value)?;
    }
    println!("{}={}", name, value);
    Ok(())
}

fn is_merge_commit(repo: &Path) -> DetectionResult<bool> {
    let output = git(repo, &["cat-file", "-p", "HEAD"])?;
    Ok(output
        .lines()
        .filter(|line| line.starts_with("parent "))
        .count()
        > 1)
}

fn get_changed_files(repo: &Path, event: &str) -> DetectionResult<Vec<String>> {
    // GitHub Actions checks out a synthetic merge commit for pull_request
    // events: HEAD is the merge commit, HEAD^ is the base branch, HEAD^2
    // is the actual PR head. To get the per-commit diff (what the latest
    // push actually changed), we compare HEAD^2^ to HEAD^2.
    // For push events, HEAD is the real commit, so HEAD^ to HEAD works.
    let output = if event == "pull_request" && is_merge_commit(repo)? {
        println!("Merge commit detected (pull_request event)");
        let parent = if git(repo, &["rev-parse", "--verify", "HEAD^2^"]).is_ok() {
            "HEAD^2^"
        } else {
            "HEAD^"
        };
        println!("Comparing {parent} to HEAD^2 (per-commit diff of PR head)");
        git(
            repo,
            &["diff", "--name-only", "--no-renames", parent, "HEAD^2"],
        )?
    } else if git(repo, &["rev-parse", "--verify", "HEAD^"]).is_ok() {
        // A real merge pushed to main must include the entire first-parent diff.
        println!("Comparing HEAD^ to HEAD");
        git(
            repo,
            &["diff", "--name-only", "--no-renames", "HEAD^", "HEAD"],
        )?
    } else {
        println!("HEAD^ not available, listing all files in HEAD");
        git(repo, &["ls-tree", "--name-only", "-r", "HEAD"])?
    };

    Ok(output
        .lines()
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect())
}

fn is_excluded_from_code_changes(file_path: &str) -> bool {
    // Exclude markdown files in any folder
    if file_path.ends_with(".md") {
        return true;
    }

    // Exclude specific folders from code changes
    let excluded_folders = ["changelog.d/", "docs/", "experiments/", "examples/"];

    for folder in &excluded_folders {
        if file_path.starts_with(folder) {
            return true;
        }
    }

    false
}

fn is_cargo_input(file_path: &str) -> bool {
    file_path.ends_with(".toml") || file_path.rsplit('/').next() == Some("Cargo.lock")
}

fn affects_workflow(file_path: &str) -> bool {
    file_path.starts_with(".github/") || file_path.starts_with("rust/scripts/")
}

fn run() -> DetectionResult<()> {
    println!("Detecting file changes for CI/CD...\n");

    let changed_files = get_changed_files(
        Path::new("."),
        &env::var("GITHUB_EVENT_NAME").unwrap_or_default(),
    )?;

    println!("Changed files:");
    if changed_files.is_empty() {
        println!("  (none)");
    } else {
        for file in &changed_files {
            println!("  {}", file);
        }
    }
    println!();

    // Detect .rs file changes (Rust source)
    let rs_changed = changed_files.iter().any(|f| f.ends_with(".rs"));
    set_output("rs-changed", if rs_changed { "true" } else { "false" })?;

    // Cargo.lock changes alter the resolved code even without a manifest edit.
    let toml_changed = changed_files.iter().any(|f| is_cargo_input(f));
    set_output("toml-changed", if toml_changed { "true" } else { "false" })?;

    // Detect .mjs file changes (scripts)
    let mjs_changed = changed_files.iter().any(|f| f.ends_with(".mjs"));
    set_output("mjs-changed", if mjs_changed { "true" } else { "false" })?;

    // Detect documentation changes (any .md file)
    let docs_changed = changed_files.iter().any(|f| f.ends_with(".md"));
    set_output("docs-changed", if docs_changed { "true" } else { "false" })?;

    // Detect workflow changes
    let workflow_changed = changed_files.iter().any(|f| affects_workflow(f));
    set_output(
        "workflow-changed",
        if workflow_changed { "true" } else { "false" },
    )?;

    // Detect code changes (excluding docs, changelog.d, experiments, examples folders, and markdown files)
    let code_changed_files: Vec<&String> = changed_files
        .iter()
        .filter(|f| !is_excluded_from_code_changes(f))
        .collect();

    println!("\nFiles considered as code changes:");
    if code_changed_files.is_empty() {
        println!("  (none)");
    } else {
        for file in &code_changed_files {
            println!("  {}", file);
        }
    }
    println!();

    // Check if any code files changed (.rs, .toml, .mjs, .yml, .yaml, or workflow files)
    let code_pattern = Regex::new(r"\.(rs|toml|mjs|js|yml|yaml)$|\.github/workflows/").unwrap();
    let code_changed = code_changed_files
        .iter()
        .any(|f| code_pattern.is_match(f) || is_cargo_input(f));
    set_output(
        "any-code-changed",
        if code_changed { "true" } else { "false" },
    )?;
    set_output(
        "rust-code-changed",
        if code_changed { "true" } else { "false" },
    )?;

    println!("\nChange detection completed.");
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("::error::Change detection failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::{affects_workflow, get_changed_files, git, is_cargo_input};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Repo(PathBuf);

    impl Repo {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "link-cli-detector-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            git(&path, &["init", "-b", "main"]).unwrap();
            git(&path, &["config", "user.name", "Regression test"]).unwrap();
            git(&path, &["config", "user.email", "test@example.invalid"]).unwrap();
            Self(path)
        }

        fn commit(&self, file: &str, content: &str) {
            let path = self.0.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
            git(&self.0, &["add", file]).unwrap();
            git(&self.0, &["commit", "-m", "Change fixture"]).unwrap();
        }
    }

    impl Drop for Repo {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn lock_only_updates_require_cargo_checks() {
        for file in [
            "Cargo.lock",
            "rust/Cargo.lock",
            "rust/wasm/Cargo.lock",
            "rust/Cargo.toml",
        ] {
            assert!(is_cargo_input(file), "{file} must trigger Cargo checks");
        }
        for file in [
            "rust/src/storage/lock.rs",
            "rust/NotCargo.lock",
            "rust/README.md",
        ] {
            assert!(!is_cargo_input(file), "{file} is not a Cargo input");
        }
    }

    #[test]
    fn shell_helper_changes_require_workflow_checks() {
        assert!(affects_workflow("rust/scripts/check-scripts.sh"));
        assert!(affects_workflow("rust/scripts/install-rust-script.sh"));
        assert!(affects_workflow(".github/scripts/check-cargo-warnings.sh"));
        assert!(!affects_workflow("rust/README.md"));
    }

    #[test]
    fn empty_diffs_and_missing_git_evidence_are_distinct() {
        let repo = Repo::new();
        assert!(get_changed_files(&repo.0, "push").is_err());
        repo.commit("rust/src/lib.rs", "pub fn example() {}\n");
        assert_eq!(
            get_changed_files(&repo.0, "push").unwrap(),
            ["rust/src/lib.rs"]
        );
        git(&repo.0, &["commit", "--allow-empty", "-m", "Empty fixture"]).unwrap();
        assert!(get_changed_files(&repo.0, "push").unwrap().is_empty());
    }

    #[test]
    fn main_merge_checks_all_changes_and_pr_keeps_per_commit_scope() {
        let repo = Repo::new();
        repo.commit("README.md", "Initial\n");
        git(&repo.0, &["checkout", "-b", "feature"]).unwrap();
        repo.commit("rust/src/lib.rs", "pub fn example() {}\n");
        repo.commit("README.md", "Documentation-only final commit\n");
        git(&repo.0, &["checkout", "main"]).unwrap();
        repo.commit("base.md", "Base advanced\n");
        git(
            &repo.0,
            &["merge", "--no-ff", "feature", "-m", "Merge fixture"],
        )
        .unwrap();
        assert!(get_changed_files(&repo.0, "push")
            .unwrap()
            .contains(&"rust/src/lib.rs".to_owned()));
        assert_eq!(
            get_changed_files(&repo.0, "pull_request").unwrap(),
            ["README.md"]
        );
    }

    #[test]
    fn moving_code_into_an_excluded_folder_keeps_source_deletion() {
        let repo = Repo::new();
        repo.commit("rust/src/lib.rs", "pub fn example() {}\n");
        fs::create_dir_all(repo.0.join("examples")).unwrap();
        git(&repo.0, &["mv", "rust/src/lib.rs", "examples/probe.rs"]).unwrap();
        git(&repo.0, &["commit", "-m", "Move fixture"]).unwrap();
        let changed = get_changed_files(&repo.0, "push").unwrap();
        assert!(changed.contains(&"rust/src/lib.rs".to_owned()));
        assert!(changed.contains(&"examples/probe.rs".to_owned()));
    }
}
