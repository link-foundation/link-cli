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
