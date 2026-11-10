//! Exact names must survive the legacy LinkStorage text file, not only its
//! separate Unicode names adapter. Names are opaque UTF-8 strings.
use anyhow::Result;
use link_cli::{Link, LinkStorage};

fn round_trip(names: &[&str]) -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("links");
    let mut store = LinkStorage::new(&path, false)?;
    let mut expected = Vec::new();
    for (position, name) in names.iter().enumerate() {
        let source = 1000 + position as u32;
        let target = 2000 + position as u32;
        let id = store.create(source, target);
        store.set_name(id, name);
        expected.push((Link::new(id, source, target), *name));
    }
    store.save()?;
    drop(store);
    for reopen in 0..2 {
        let store = LinkStorage::new(&path, false)?;
        assert_eq!(store.all().len(), expected.len(), "reopen {reopen}");
        for (link, name) in &expected {
            assert_eq!(store.get(link.index), Some(link));
            assert_eq!(
                store.get_name(link.index).map(String::as_str),
                Some(*name),
                "reopen {reopen}, {name:?}"
            );
            assert_eq!(store.get_by_name(name), Some(link.index));
        }
        store.save()?;
    }
    Ok(())
}

#[test]
fn inline_names_preserve_whitespace() -> Result<()> {
    round_trip(&[
        "leaf 雪",
        " leading",
        "trailing ",
        " both ",
        "a\tb",
        "\u{2028}",
    ])
}

#[test]
fn inline_names_preserve_quote_edges_and_parentheses() -> Result<()> {
    round_trip(&["\"quoted\"", "\"", "a\"b", "a'`b", ")", "((end))"])
}

#[test]
fn inline_names_keep_line_breaks_inside_the_name() -> Result<()> {
    round_trip(&["a\n(99 99 99)\nz", "\r", "a\r\nb", "\n", "tail\r"])
}

#[test]
fn inline_names_preserve_literal_looking_text_and_controls() -> Result<()> {
    round_trip(&[
        "",
        "~1{31}",
        "~2{}",
        "# unused: 3",
        r"a\nb",
        "\0a\0",
        "\u{feff}雪",
        "a\u{7f}b",
        "😀",
    ])
}

#[test]
fn legacy_inline_names_keep_backslashes_and_the_complete_quoted_tail() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("links");
    let original = "# legacy file\n(1 10 11 \"a b\")\n(2 12 13 \"a\\nb\")\n(3 14 15 \"~1{31}\")\n(4 16 17 \"\"quoted\"\")\n(5 18 19)\n";
    std::fs::write(&path, original)?;
    let store = LinkStorage::new(&path, false)?;
    for (id, name) in [(1, "a b"), (2, r"a\nb"), (3, "~1{31}"), (4, "\"quoted\"")] {
        assert_eq!(store.get_name(id).map(String::as_str), Some(name));
        assert_eq!(store.get_by_name(name), Some(id));
    }
    assert_eq!(store.get_name(5), None);
    assert_eq!(
        std::fs::read_to_string(&path)?,
        original,
        "opening must not rewrite the file"
    );
    Ok(())
}

#[test]
fn names_of_absent_addresses_round_trip_without_allocating_rows() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("links");
    let names = [(0, "zero"), (77, "absent 雪"), (u32::MAX, "external\nname")];
    let mut store = LinkStorage::new(&path, false)?;
    for (id, name) in names {
        store.set_name(id, name);
    }
    store.save()?;
    drop(store);
    for _ in 0..2 {
        let store = LinkStorage::new(&path, false)?;
        assert!(store.all().is_empty());
        for (id, name) in names {
            assert!(!store.exists(id));
            assert_eq!(store.get_name(id).map(String::as_str), Some(name));
            assert_eq!(store.get_by_name(name), Some(id));
        }
        store.save()?;
    }
    let mut store = LinkStorage::new(&path, false)?;
    assert_eq!(
        store.create(0, 0),
        1,
        "name metadata must not advance allocation"
    );
    Ok(())
}

#[test]
fn failed_reload_preserves_live_rows_names_indexes_and_allocator() -> Result<()> {
    for indexed in [false, true] {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("links");
        let mut store = LinkStorage::new(&path, false)?;
        if indexed {
            store = store.with_indexed_lookups();
        }
        let first = store.create(10, 11);
        let freed = store.create(20, 21);
        let last = store.create(30, 31);
        store.delete(freed)?;
        store.set_name(first, "first");
        store.set_name(last, "last");
        store.save()?;
        let original = std::fs::read(&path)?;
        std::fs::write(&path, b"(9 90 91)\n\xff")?;
        assert!(store.reload_from_disk().is_err());
        assert_eq!(store.all().len(), 2);
        assert_eq!(store.search(10, 11), Some(first));
        assert_eq!(store.search(30, 31), Some(last));
        assert!(!store.exists(9));
        assert_eq!(store.get_by_name("first"), Some(first));
        assert_eq!(store.get_by_name("last"), Some(last));
        assert_eq!(std::fs::read(&path)?, b"(9 90 91)\n\xff");
        std::fs::write(&path, original)?;
        store.reload_from_disk()?;
        assert_eq!(store.create(40, 41), freed);
        store.save()?;
        let mut reopened = LinkStorage::new(&path, false)?;
        assert_eq!(reopened.get_by_name("first"), Some(first));
        assert_eq!(reopened.create(50, 51), last + 1);
    }
    Ok(())
}

#[test]
fn independent_metadata_vectors_preserve_names_and_following_edits() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("links");
    // Authored bytes independently of the production literal encoder.
    let canonical = "(1 10 11)\n(2 12 13 \"a b\")\n# link-cli:name:1 0 ~1{}\n# link-cli:name:1 1 ~1{610a6222e99baa}\n# link-cli:name:1 4294967295 ~1{efbbbf00}\n";
    std::fs::write(&path, canonical)?;
    for _ in 0..2 {
        let store = LinkStorage::new(&path, false)?;
        assert_eq!(store.all().len(), 2);
        for (id, name) in [
            (0, ""),
            (1, "a\nb\"雪"),
            (2, "a b"),
            (u32::MAX, "\u{feff}\0"),
        ] {
            assert_eq!(store.get_name(id).map(String::as_str), Some(name));
            assert_eq!(store.get_by_name(name), Some(id));
        }
        store.save()?;
        assert_eq!(std::fs::read_to_string(&path)?, canonical);
    }
    let mut store = LinkStorage::new(&path, false)?;
    let next = store.create(30, 31);
    assert_eq!(next, 3);
    store.set_name(1, "renamed 雪");
    store.remove_name(0);
    store.set_name(next, "\u{feff}\0"); // Move the external address's name.
    store.remove_name(2);
    store.save()?;
    let mut store = LinkStorage::new(&path, false)?;
    assert_eq!(store.get_by_name("renamed 雪"), Some(1));
    assert_eq!(store.get_name(0), None);
    assert_eq!(store.get_name(2), None);
    assert_eq!(store.get_name(u32::MAX), None);
    assert_eq!(store.get_by_name("\u{feff}\0"), Some(next));
    store.delete(1)?;
    assert_eq!(store.create(40, 41), 1);
    store.save()?;
    let mut store = LinkStorage::new(&path, false)?;
    assert_eq!(
        store.get_name(1),
        None,
        "reused address must not regain a removed name"
    );
    assert_eq!(store.get_by_name("renamed 雪"), None);
    assert_eq!(store.get_by_name("a\nb\"雪"), None);
    assert_eq!(store.get_by_name("\u{feff}\0"), Some(next));
    assert_eq!(store.create(50, 51), 4);
    Ok(())
}

#[test]
fn malformed_or_future_name_metadata_preserves_files_and_live_state() -> Result<()> {
    let invalid = [
        "",
        "1",
        "1 1",
        "1 1 ~1{} extra",
        "2 1 ~1{}",
        "01 1 ~1{}",
        "1 -1 ~1{}",
        "1 +1 ~1{}",
        "1 no ~1{}",
        "1 4294967296 ~1{}",
        "1 1 bare",
        "1 1 ~2{}",
        "1 1 ~1{0}",
        "1 1 ~1{zz}",
        "1 1 ~1{ff}",
        "1 1 ~1{eda080}",
        "1 1 ~1{c3}",
        "1 1 ~1{00",
    ];
    for indexed in [false, true] {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("links");
        let mut store = LinkStorage::new(&path, false)?;
        if indexed {
            store = store.with_indexed_lookups();
        }
        let id = store.create(10, 11);
        store.set_name(id, "stable");
        store.save()?;
        let original = std::fs::read(&path)?;
        for metadata in invalid {
            let bytes = format!("(9 90 91)\n# link-cli:name:{metadata}\n");
            std::fs::write(&path, &bytes)?;
            let error = LinkStorage::new(&path, false)
                .err()
                .expect("invalid metadata opened");
            let message = format!("{error:#}");
            assert!(
                message.contains("name metadata") && message.contains("line 2"),
                "{message}"
            );
            assert!(message.contains(path.to_str().unwrap()), "{message}");
            assert!(store.reload_from_disk().is_err());
            assert_eq!(store.search(10, 11), Some(id));
            assert_eq!(store.get_by_name("stable"), Some(id));
            assert_eq!(store.all().len(), 1);
            assert!(!store.exists(9));
            assert_eq!(std::fs::read_to_string(&path)?, bytes);
        }
        std::fs::write(&path, original)?;
        store.reload_from_disk()?;
        assert_eq!(store.create(20, 21), 2);
    }
    Ok(())
}

#[test]
fn bounded_mixed_unicode_names_round_trip_exactly() -> Result<()> {
    let alphabet = [
        ' ', '\t', '\r', '\n', '\0', '\u{7f}', '\u{85}', '\u{2028}', '\u{feff}', '雪', '😀', '"',
        '\'', '`', '(', ')', '~', '\\',
    ];
    let mut state = 0x13_800_u32;
    let names = (0..64)
        .map(|index| {
            let mut name = format!("{index}:");
            for _ in 0..12 {
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                name.push(alphabet[state as usize % alphabet.len()]);
            }
            name
        })
        .collect::<Vec<_>>();
    round_trip(&names.iter().map(String::as_str).collect::<Vec<_>>())
}

#[test]
fn legacy_unquoted_literals_and_ignored_rows_keep_their_meaning() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("links");
    let bytes = "# legacy\r\n\r\n()\r\n(1 10 11 \"quoted tail 雪\")\r\n2 12 13 ~1{31}\n(3 14 15 bare ignored)\n(4 16 17 \"unterminated)\n(((5 20 21 \"nested row\")))\ninvalid\n(no 1 2)\n(6 no 10)\n(7 10 no)\n(8 10)\n# link-cli:other:2 1 anything\n";
    std::fs::write(&path, bytes)?;
    let store = LinkStorage::new(&path, false)?;
    assert_eq!(store.all().len(), 5);
    for (id, name) in [
        (1, "quoted tail 雪"),
        (2, "~1{31}"),
        (3, "bare"),
        (4, "unterminated"),
        (5, "nested row"),
    ] {
        assert_eq!(store.get_name(id).map(String::as_str), Some(name));
    }
    assert_eq!(std::fs::read_to_string(&path)?, bytes);
    Ok(())
}
