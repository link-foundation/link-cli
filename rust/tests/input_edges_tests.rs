//! Refusals and rare forms of the LiNo import and of external ids.

use link_cli::{import_lino_text, LinkStorage, LinksExternalIds, NamedTypeLinks};

fn store() -> LinkStorage {
    LinkStorage::in_memory(false)
}

fn refusal(text: &str) -> String {
    import_lino_text(&mut store(), text)
        .expect_err(text)
        .to_string()
}

#[test]
fn the_import_refuses_forms_it_cannot_store() {
    assert!(refusal("(1 1)").contains("must have an index or name"));
    assert!(refusal("(a: (1 1 1) 2)").contains("must be scalar values or nested links"));
}

#[test]
fn the_import_takes_an_id_that_is_quoted_inside_its_quotes() {
    for (text, name) in [
        ("('\"inner\"': 1 1)", "inner"),
        ("(\"'other'\": 1 1)", "other"),
    ] {
        let mut storage = store();
        import_lino_text(&mut storage, text).unwrap();
        assert!(storage.get_by_name(name).is_some(), "{text}");
    }
}

#[test]
fn importing_the_same_link_twice_changes_nothing() {
    let mut storage = store();
    import_lino_text(&mut storage, "(5: 1 1)").unwrap();
    import_lino_text(&mut storage, "(5: 1 1)").unwrap();
    let link = storage.get_link(5).unwrap();
    assert_eq!((link.source, link.target), (1, 1));
}

#[test]
fn an_id_table_refuses_what_it_cannot_name() {
    assert!(LinksExternalIds::new("").is_err());
    assert!(LinksExternalIds::new("a:b").is_err());
    let ids = LinksExternalIds::new("uuid").unwrap();
    assert_eq!(ids.scheme(), "uuid");
    assert!(ids.name_of("").is_err());
    assert_eq!(ids.name_of("x").unwrap(), "uuid:x");
    assert!(ids.bind(&mut store(), "x", 0).is_err());
}

#[test]
fn only_the_scheme_the_table_holds_answers() {
    let (mut storage, ids) = (store(), LinksExternalIds::new("uuid").unwrap());
    let link = storage.try_create(1, 1).unwrap();
    assert_eq!(ids.id_of(&mut storage, link).unwrap(), None);

    ids.bind(&mut storage, "x", link).unwrap();
    assert_eq!(ids.id_of(&mut storage, 0).unwrap(), None);
    let other = storage.try_create(2, 2).unwrap();
    assert_eq!(ids.id_of(&mut storage, other).unwrap(), None);

    // Another scheme of the same table has no link yet.
    let urls = LinksExternalIds::new("url").unwrap();
    assert_eq!(urls.id_of(&mut storage, link).unwrap(), None);

    // A link that only carries the name of a scheme is not that scheme.
    let isbn = LinksExternalIds::new("isbn").unwrap();
    storage.set_name(other, "isbn");
    assert_eq!(isbn.id_of(&mut storage, link).unwrap(), None);
    assert_eq!(isbn.find(&mut storage, "x").unwrap(), None);
}
