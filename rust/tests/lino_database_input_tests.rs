use anyhow::Result;
use link_cli::{import_lino_text, LinkStorage, NamedTypeLinks};
use tempfile::NamedTempFile;

#[test]
fn import_lino_text_reproduces_numbered_links_at_explicit_indexes() -> Result<()> {
    let temp_file = NamedTempFile::new()?;
    let db_path = temp_file.path().to_str().unwrap();
    let mut storage = LinkStorage::new(db_path, false)?;

    import_lino_text(
        &mut storage,
        r#"
        (1: 1 1)
        (2: 1 2)
        (3: 2 1)
        "#,
    )?;

    assert_eq!(
        storage.lino_lines(),
        vec!["(1: 1 1)", "(2: 1 2)", "(3: 2 1)"]
    );

    Ok(())
}

#[test]
fn import_lino_text_creates_named_references_as_point_links() -> Result<()> {
    let temp_file = NamedTempFile::new()?;
    let db_path = temp_file.path().to_str().unwrap();
    let mut storage = LinkStorage::new(db_path, false)?;

    import_lino_text(&mut storage, "(child: father mother)")?;

    let father = storage.get_by_name("father").expect("father should exist");
    let mother = storage.get_by_name("mother").expect("mother should exist");
    let child = storage.get_by_name("child").expect("child should exist");

    let father_link = storage.get_link(father).expect("father link should exist");
    let mother_link = storage.get_link(mother).expect("mother link should exist");
    let child_link = storage.get_link(child).expect("child link should exist");

    assert_eq!(father_link.source, father);
    assert_eq!(father_link.target, father);
    assert_eq!(mother_link.source, mother);
    assert_eq!(mother_link.target, mother);
    assert_eq!(child_link.source, father);
    assert_eq!(child_link.target, mother);

    let lines = storage.lino_lines();
    assert!(lines.contains(&"(father: father father)".to_string()));
    assert!(lines.contains(&"(mother: mother mother)".to_string()));
    assert!(lines.contains(&"(child: father mother)".to_string()));

    Ok(())
}

#[test]
fn import_lino_text_treats_out_of_range_numbers_as_names() -> Result<()> {
    let temp_file = NamedTempFile::new()?;
    let db_path = temp_file.path().to_str().unwrap();
    let mut storage = LinkStorage::new(db_path, false)?;
    let out_of_range_number = u64::from(u32::MAX) + 1;

    // Target `2` rather than `1` for the same reason the C# counterpart
    // (`LinoDatabaseInputTests.ImportText_TreatsOutOfRangeNumbersAsNames`)
    // uses it: the out-of-range number becomes the point link `(1: 1 1)`,
    // so `(child: <number> 1)` would ask for a second `(1, 1)` link and
    // uniqueness resolution would merge `child` into it.
    import_lino_text(&mut storage, &format!("(child: {out_of_range_number} 2)"))?;

    let numeric_name = storage
        .get_by_name(&out_of_range_number.to_string())
        .expect("out-of-range number should be named");
    let child = storage.get_by_name("child").expect("child should exist");

    let numeric_name_link = storage
        .get_link(numeric_name)
        .expect("named number link should exist");
    let child_link = storage.get_link(child).expect("child link should exist");

    assert_eq!(numeric_name_link.source, numeric_name);
    assert_eq!(numeric_name_link.target, numeric_name);
    assert_eq!(child_link.source, numeric_name);

    Ok(())
}
