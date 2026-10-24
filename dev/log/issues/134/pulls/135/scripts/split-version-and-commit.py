"""Move the changelog helpers of rust/scripts/version-and-commit.rs into
rust/scripts/release-changelog.rs (keeps both files under the 1000-line limit)."""
import re, sys
root = sys.argv[1]
p = f'{root}/rust/scripts/version-and-commit.rs'
s = open(p).read()

def cut(text, start, end):
    i = text.index(start); j = text.index(end, i)
    return text[:i] + text[j:], text[i:j]

s, consts = cut(s, '/// Marker that `collect-changelog.rs`', 'fn get_arg(')
s, helpers = cut(s, 'fn strip_frontmatter(', '/// The version commit is pushed straight')

tests_names = [
    'collect_creates_a_missing_changelog_and_consumes_fragments',
    'collect_inserts_the_new_release_above_the_previous_one',
    'collect_without_fragments_changes_nothing',
    'collect_rejects_fragments_without_a_description',
]
moved_tests = []
for name in tests_names:
    start = s.index(f'    #[test]\n    fn {name}(')
    end = s.index('    #[test]\n', start + 10)
    moved_tests.append(s[start:end])
    s = s[:start] + s[end:]

s = s.replace('mod rust_paths;\n', 'mod rust_paths;\n\n#[path = "release-changelog.rs"]\nmod release_changelog;\n\nuse release_changelog::{collect_changelog_with_date, list_fragments};\n', 1)
# tests in the main script still use the creation helper for fixtures
s = s.replace('    use super::*;\n', '    use super::*;\n    use release_changelog::insert_changelog_entry;\n', 1)
open(p, 'w').write(s)

new_insert = '''/// Insert `entry` above the newest release (after the insert marker when the
/// file has one); a missing changelog is created with the standard header.
/// Exactly one blank line separates the entry from its neighbours, however
/// many blank lines the entry or the file already carry.
pub fn insert_changelog_entry(existing: Option<&str>, entry: &str) -> String {
    let entry = entry.trim();
    let content = match existing {
        Some(content) => content.to_string(),
        None => format!("{}\\n{}\\n", CHANGELOG_HEADER, INSERT_MARKER),
    };

    if let Some(idx) = content.find(INSERT_MARKER) {
        let (before, after) = content.split_at(idx + INSERT_MARKER.len());
        let after = after.trim_start();
        return if after.is_empty() {
            format!("{}\\n\\n{}\\n", before, entry)
        } else {
            format!("{}\\n\\n{}\\n\\n{}", before, entry, after)
        };
    }

    let heading = content
        .match_indices("## [")
        .map(|(idx, _)| idx)
        .find(|&idx| idx == 0 || content[..idx].ends_with('\\n'));
    match heading {
        Some(idx) => {
            let (before, rest) = content.split_at(idx);
            format!("{}\\n\\n{}\\n\\n{}", before.trim_end(), entry, rest)
        }
        None => format!("{}\\n\\n{}\\n", content.trim_end(), entry),
    }
}
'''
helpers = re.sub(r'/// Insert `entry` above the newest release.*?\n}\n', lambda m: new_insert, helpers, count=1, flags=re.S)
helpers = helpers.replace('fn strip_frontmatter(', 'pub fn strip_frontmatter(', 1)
helpers = helpers.replace('\nfn list_fragments(', '\npub fn list_fragments(', 1)
helpers = helpers.replace('\nfn collect_changelog_with_date(', '\npub fn collect_changelog_with_date(', 1)
consts = consts.replace('const INSERT_MARKER', 'pub const INSERT_MARKER').replace('const CHANGELOG_HEADER', 'pub const CHANGELOG_HEADER')

spacing_test = '''    #[test]
    fn repeated_inserts_keep_exactly_one_blank_line() {
        // The C# CHANGELOG had gained one blank line per release (issue #134).
        let once = insert_changelog_entry(None, "\\n## [1.0.0] - 2026-01-01\\n\\nFirst.\\n");
        let twice = insert_changelog_entry(Some(&once), "\\n## [1.0.1] - 2026-01-02\\n\\nSecond.\\n");
        assert!(!twice.contains("\\n\\n\\n"), "{twice:?}");
        assert!(twice.ends_with(&format!(
            "{INSERT_MARKER}\\n\\n## [1.0.1] - 2026-01-02\\n\\nSecond.\\n\\n## [1.0.0] - 2026-01-01\\n\\nFirst.\\n"
        )));

        let grown = format!("# Changelog\\n\\nIntro.\\n{}## [1.0.0] - 2026-01-01\\n\\nOld.\\n", "\\n".repeat(12));
        assert_eq!(
            insert_changelog_entry(Some(&grown), "## [1.0.1] - 2026-01-02\\n\\nNew."),
            "# Changelog\\n\\nIntro.\\n\\n## [1.0.1] - 2026-01-02\\n\\nNew.\\n\\n## [1.0.0] - 2026-01-01\\n\\nOld.\\n"
        );
    }

'''

module = '''//! Changelog fragment collection shared by `version-and-commit.rs`.
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

''' + consts.rstrip() + '\n\n' + helpers.rstrip() + '''

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
        fs::write(dir.join(name), format!("---\\nbump: patch\\n---\\n\\n{body}\\n")).unwrap();
    }

''' + spacing_test + ''.join(moved_tests).rstrip() + '\n}\n'
open(f'{root}/rust/scripts/release-changelog.rs', 'w').write(module)
