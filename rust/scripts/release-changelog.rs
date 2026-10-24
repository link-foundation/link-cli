//! Changelog fragment collection shared by `version-and-commit.rs`.
//!
//! Included with `#[path = "release-changelog.rs"] mod release_changelog;`;
//! its tests run through `rust-script --test rust/scripts/version-and-commit.rs`.
//! A release consumes every `changelog.d/*.md` fragment into a new
//! `## [version] - date` section of CHANGELOG.md (created when missing, see
//! issue #134) and deletes the consumed fragments.

#![allow(dead_code)]

use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};

/// Marker that `collect-changelog.rs` also inserts new entries after.
pub const INSERT_MARKER: &str = "<!-- changelog-insert-here -->";

pub const CHANGELOG_HEADER: &str = "# Changelog\n\n\
All notable changes to this project will be documented in this file.\n\n\
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),\n\
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).\n";

pub fn strip_frontmatter(content: &str) -> String {
    let re = Regex::new(r"(?s)^---\s*\n.*?\n---\s*\n(.*)$").unwrap();
    if let Some(caps) = re.captures(content) {
        caps.get(1).unwrap().as_str().trim().to_string()
    } else {
        content.trim().to_string()
    }
}

/// Sorted `.md` fragment paths in `changelog_dir`, excluding README.md.
/// A missing directory has no fragments; any other I/O failure is an error
/// instead of an empty list.
pub fn list_fragments(changelog_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = match fs::read_dir(changelog_dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("Failed to read {}: {}", changelog_dir.display(), e)),
    };
    let mut files = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|e| format!("Failed to read {}: {}", changelog_dir.display(), e))?
            .path();
        if path.extension().map_or(false, |ext| ext == "md")
            && path.file_name().map_or(false, |name| name != "README.md")
        {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

/// Insert `entry` above the newest release (after the insert marker when the
/// file has one); a missing changelog is created with the standard header.
/// Exactly one blank line separates the entry from its neighbours, however
/// many blank lines the entry or the file already carry.
pub fn insert_changelog_entry(existing: Option<&str>, entry: &str) -> String {
    let entry = entry.trim();
    let content = match existing {
        Some(content) => content.to_string(),
        None => format!("{}\n{}\n", CHANGELOG_HEADER, INSERT_MARKER),
    };

    if let Some(idx) = content.find(INSERT_MARKER) {
        let (before, after) = content.split_at(idx + INSERT_MARKER.len());
        let after = after.trim_start();
        return if after.is_empty() {
            format!("{}\n\n{}\n", before, entry)
        } else {
            format!("{}\n\n{}\n\n{}", before, entry, after)
        };
    }

    let heading = content
        .match_indices("## [")
        .map(|(idx, _)| idx)
        .find(|&idx| idx == 0 || content[..idx].ends_with('\n'));
    match heading {
        Some(idx) => {
            let (before, rest) = content.split_at(idx);
            format!("{}\n\n{}\n\n{}", before.trim_end(), entry, rest)
        }
        None => format!("{}\n\n{}\n", content.trim_end(), entry),
    }
}

/// Move every changelog fragment into a new `## [version] - date` section of
/// CHANGELOG.md and delete the consumed fragments, so the next release does
/// not publish them again. Returns the number of consumed fragments.
pub fn collect_changelog_with_date(
    changelog_dir: &str,
    changelog_file: &str,
    version: &str,
    date_str: &str,
) -> Result<usize, String> {
    let files = list_fragments(Path::new(changelog_dir))?;
    if files.is_empty() {
        println!("No changelog fragments in {}", changelog_dir);
        return Ok(0);
    }

    let mut fragments = Vec::new();
    let mut empty = Vec::new();
    for file in &files {
        let content = fs::read_to_string(file)
            .map_err(|e| format!("Failed to read {}: {}", file.display(), e))?;
        let body = strip_frontmatter(&content);
        if body.is_empty() {
            empty.push(file.display().to_string());
        } else {
            fragments.push(body);
        }
    }
    if !empty.is_empty() {
        return Err(format!(
            "Changelog fragments without a description cannot be released: {}",
            empty.join(", ")
        ));
    }

    let entry = format!(
        "\n## [{}] - {}\n\n{}\n",
        version,
        date_str,
        fragments.join("\n\n")
    );

    let existing = if Path::new(changelog_file).exists() {
        Some(
            fs::read_to_string(changelog_file)
                .map_err(|e| format!("Failed to read {}: {}", changelog_file, e))?,
        )
    } else {
        println!("Creating {} (it did not exist yet)", changelog_file);
        None
    };
    let content = insert_changelog_entry(existing.as_deref(), &entry);
    fs::write(changelog_file, content)
        .map_err(|e| format!("Failed to write {}: {}", changelog_file, e))?;

    for file in &files {
        fs::remove_file(file).map_err(|e| format!("Failed to remove {}: {}", file.display(), e))?;
        println!("Removed {}", file.display());
    }
    println!("Collected {} changelog fragment(s)", files.len());
    Ok(files.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = env::temp_dir().join(format!("release-changelog-{name}-{nanos}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn write_fragment(dir: &Path, name: &str, body: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join(name), format!("---\nbump: patch\n---\n\n{body}\n")).unwrap();
    }

    #[test]
    fn repeated_inserts_keep_exactly_one_blank_line() {
        // The C# CHANGELOG had gained one blank line per release (issue #134).
        let once = insert_changelog_entry(None, "\n## [1.0.0] - 2026-01-01\n\nFirst.\n");
        let twice = insert_changelog_entry(Some(&once), "\n## [1.0.1] - 2026-01-02\n\nSecond.\n");
        assert!(!twice.contains("\n\n\n"), "{twice:?}");
        assert!(twice.ends_with(&format!(
            "{INSERT_MARKER}\n\n## [1.0.1] - 2026-01-02\n\nSecond.\n\n## [1.0.0] - 2026-01-01\n\nFirst.\n"
        )));

        let grown = format!(
            "# Changelog\n\nIntro.\n{}## [1.0.0] - 2026-01-01\n\nOld.\n",
            "\n".repeat(12)
        );
        assert_eq!(
            insert_changelog_entry(Some(&grown), "## [1.0.1] - 2026-01-02\n\nNew."),
            "# Changelog\n\nIntro.\n\n## [1.0.1] - 2026-01-02\n\nNew.\n\n## [1.0.0] - 2026-01-01\n\nOld.\n"
        );
    }

    #[test]
    fn collect_creates_a_missing_changelog_and_consumes_fragments() {
        // Regression for issue #134: rust/CHANGELOG.md did not exist, so the
        // fragments were never written anywhere and never removed.
        let root = temp_dir("missing-changelog");
        let dir = root.join("changelog.d");
        write_fragment(&dir, "20260101_000000_a.md", "First change.");
        write_fragment(&dir, "20260102_000000_b.md", "Second change.");
        fs::write(dir.join("README.md"), "# Changelog Fragments\n").unwrap();
        let changelog = root.join("CHANGELOG.md");

        let consumed = collect_changelog_with_date(
            dir.to_str().unwrap(),
            changelog.to_str().unwrap(),
            "1.0.9",
            "2026-10-24",
        )
        .unwrap();

        assert_eq!(consumed, 2);
        let content = fs::read_to_string(&changelog).unwrap();
        assert!(content.starts_with("# Changelog\n"));
        assert!(content.contains("## [1.0.9] - 2026-10-24\n\nFirst change.\n\nSecond change.\n"));
        assert!(list_fragments(&dir).unwrap().is_empty());
        assert!(dir.join("README.md").exists());
    }

    #[test]
    fn collect_inserts_the_new_release_above_the_previous_one() {
        let root = temp_dir("existing-changelog");
        let dir = root.join("changelog.d");
        write_fragment(&dir, "20260103_000000_c.md", "Newest change.");
        let changelog = root.join("CHANGELOG.md");
        let old = insert_changelog_entry(None, "\n## [1.0.0] - 2026-01-01\n\nOld.\n");
        fs::write(&changelog, &old).unwrap();

        collect_changelog_with_date(
            dir.to_str().unwrap(),
            changelog.to_str().unwrap(),
            "1.1.0",
            "2026-10-24",
        )
        .unwrap();

        let content = fs::read_to_string(&changelog).unwrap();
        let new_pos = content.find("## [1.1.0]").unwrap();
        let old_pos = content.find("## [1.0.0]").unwrap();
        assert!(new_pos < old_pos, "{content}");
        assert_eq!(content.matches(INSERT_MARKER).count(), 1);
    }

    #[test]
    fn collect_without_fragments_changes_nothing() {
        let root = temp_dir("no-fragments");
        let changelog = root.join("CHANGELOG.md");
        let consumed = collect_changelog_with_date(
            root.join("changelog.d").to_str().unwrap(),
            changelog.to_str().unwrap(),
            "1.0.1",
            "2026-10-24",
        )
        .unwrap();
        assert_eq!(consumed, 0);
        assert!(!changelog.exists());
    }

    #[test]
    fn collect_rejects_fragments_without_a_description() {
        let root = temp_dir("empty-fragment");
        let dir = root.join("changelog.d");
        write_fragment(&dir, "20260101_000000_empty.md", "");
        let error = collect_changelog_with_date(
            dir.to_str().unwrap(),
            root.join("CHANGELOG.md").to_str().unwrap(),
            "1.0.1",
            "2026-10-24",
        )
        .unwrap_err();
        assert!(error.contains("20260101_000000_empty.md"), "{error}");
        assert_eq!(list_fragments(&dir).unwrap().len(), 1);
    }
}
