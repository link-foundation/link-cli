use anyhow::Result;
use link_cli::{
    link_reference_validator::LinkReferenceValidator, LinkStorage, LinoLink, QueryProcessor,
};
use tempfile::NamedTempFile;

#[test]
fn quoted_numbers_are_names() -> Result<()> {
    for name in ["0", "1", "2", "99", "01", "+1", "4294967295"] {
        let file = NamedTempFile::new()?;
        let mut storage = LinkStorage::new(file.path().to_str().unwrap(), false)?;
        let strict = QueryProcessor::new(false);
        let auto = QueryProcessor::new(false).with_auto_create_missing_references(true);
        strict.process_query(&mut storage, "() ((a: a a) (b: b b))")?;
        for quote in ['"', '\'', '`'] {
            auto.process_query(&mut storage, &format!("() ((p: a {quote}{name}{quote}))"))?;
            let named = storage.get_by_name(name).expect("quoted name must exist");
            assert_ne!(named, storage.get_by_name("a").unwrap());
            assert_ne!(named, storage.get_by_name("b").unwrap());
            let p = storage.get_by_name("p").unwrap();
            assert_eq!(storage.get(p).unwrap().target, named);
        }
        strict.process_query(&mut storage, "() ((address: a 1))")?;
        let address = storage.get_by_name("address").unwrap();
        assert_eq!(storage.get(address).unwrap().target, 1);
    }
    Ok(())
}

#[test]
fn quoted_definitions_support_strict_matching_updating_and_deletion() -> Result<()> {
    let file = NamedTempFile::new()?;
    let mut storage = LinkStorage::new(file.path().to_str().unwrap(), false)?;
    let strict = QueryProcessor::new(false);
    strict.process_query(
        &mut storage,
        "() ((a: a a) (b: b b) (\"1\": \"1\" \"1\") ('0': '0' '0'))",
    )?;
    let one = storage.get_by_name("1").unwrap();
    strict.process_query(&mut storage, "() ((p: a \"1\"))")?;
    strict.process_query(&mut storage, "((p: a '1')) ((p: b '0'))")?;
    let p = storage.get_by_name("p").unwrap();
    assert_eq!(
        storage.get(p).unwrap().source,
        storage.get_by_name("b").unwrap()
    );
    assert_eq!(
        storage.get(p).unwrap().target,
        storage.get_by_name("0").unwrap()
    );
    strict.process_query(&mut storage, "((\"1\": '1' `1`)) ((\"1\": a '0'))")?;
    assert_eq!(storage.get_by_name("1"), Some(one));
    assert_eq!(
        storage.get(one).unwrap().target,
        storage.get_by_name("0").unwrap()
    );
    strict.process_query(&mut storage, "((p: b \"0\")) ()")?;
    assert!(!storage.exists(p));
    assert!(storage.exists(1));
    Ok(())
}

#[test]
fn missing_quoted_number_requires_its_name_even_when_address_exists() -> Result<()> {
    let file = NamedTempFile::new()?;
    let mut storage = LinkStorage::new(file.path().to_str().unwrap(), false)?;
    let strict = QueryProcessor::new(false);
    strict.process_query(&mut storage, "() ((a: a a))")?;
    let error = strict
        .process_query(&mut storage, "() ((p: a \"1\"))")
        .unwrap_err();
    assert!(error.to_string().contains("'1'"));
    assert!(error
        .to_string()
        .contains("--auto-create-missing-references"));
    assert_eq!(storage.get_by_name("1"), None);
    Ok(())
}

#[test]
fn numeric_name_remains_stable_when_its_address_is_allocated_later() -> Result<()> {
    let file = NamedTempFile::new()?;
    let mut storage = LinkStorage::new(file.path().to_str().unwrap(), false)?;
    let auto = QueryProcessor::new(false).with_auto_create_missing_references(true);
    let strict = QueryProcessor::new(false);
    auto.process_query(&mut storage, "() ((p: x '99'))")?;
    let named = storage.get_by_name("99").unwrap();
    auto.process_query(&mut storage, "() ((99: 99 99))")?;
    assert_ne!(named, 99);
    strict.process_query(&mut storage, "() ((p: x \"99\"))")?;
    assert_eq!(storage.get_by_name("99"), Some(named));
    let p = storage.get_by_name("p").unwrap();
    assert_eq!(storage.get(p).unwrap().target, named);
    strict.process_query(&mut storage, "() ((address: x 99))")?;
    let address = storage.get_by_name("address").unwrap();
    assert_eq!(storage.get(address).unwrap().target, 99);
    Ok(())
}

#[test]
fn quoted_and_encoded_references_preserve_literal_names() -> Result<()> {
    for name in [
        "*",
        "$value",
        "a:",
        " 1 ",
        r"x\y",
        "雪",
        "~1{31}",
        "\0one\0",
        "\"one\"",
        "both'\"quotes",
    ] {
        let file = NamedTempFile::new()?;
        let mut storage = LinkStorage::new(file.path().to_str().unwrap(), false)?;
        let auto = QueryProcessor::new(false).with_auto_create_missing_references(true);
        let strict = QueryProcessor::new(false);
        let literal = links_notation::encode_reference_literal(name);
        auto.process_query(&mut storage, &format!("() ((p: x {literal}))"))?;
        let named = storage.get_by_name(name).expect("literal name must exist");
        let p = storage.get_by_name("p").unwrap();
        assert_eq!(storage.get(p).unwrap().target, named);
        if !name.contains('\0') {
            let quote = if name.contains('\'') { '`' } else { '\'' };
            strict.process_query(&mut storage, &format!("() ((p: x {quote}{name}{quote}))"))?;
            assert_eq!(storage.get_by_name(name), Some(named));
        }
    }
    Ok(())
}

#[test]
fn query_quotes_follow_notation_delimiter_rules() -> Result<()> {
    for (reference, name) in [
        ("''1''", "1"),
        ("``1``", "1"),
        ("\"\"\"1\"\"\"", "1"),
        ("'a''b'", "a'b"),
        ("''a''''b''", "a''b"),
        ("'line\nnext'", "line\nnext"),
        ("'# literal'", "# literal"),
        ("'unclosed", "'unclosed"),
        ("\"unclosed", "\"unclosed"),
        ("`unclosed", "`unclosed"),
    ] {
        let file = NamedTempFile::new()?;
        let mut storage = LinkStorage::new(file.path().to_str().unwrap(), false)?;
        let auto = QueryProcessor::new(false).with_auto_create_missing_references(true);
        auto.process_query(
            &mut storage,
            &format!("() ((p: x {reference})) # comment\n"),
        )?;
        let named = storage.get_by_name(name).expect("quoted name must exist");
        let p = storage.get_by_name("p").unwrap();
        assert_eq!(storage.get(p).unwrap().target, named);
    }
    Ok(())
}

#[test]
fn exported_numeric_names_can_be_replayed_as_a_query() -> Result<()> {
    let source_file = NamedTempFile::new()?;
    let mut source = LinkStorage::new(source_file.path().to_str().unwrap(), false)?;
    let strict = QueryProcessor::new(false);
    strict.process_query(
        &mut source,
        "() ((1: 1 1) ('1': '1' '1') ('0': '0' '0') ('42': '42' '42') (p: 1 '1'))",
    )?;
    let exported = source.lino_lines();
    let destination_file = NamedTempFile::new()?;
    let mut destination = LinkStorage::new(destination_file.path().to_str().unwrap(), false)?;
    strict.process_query(&mut destination, &format!("() ({})", exported.join(" ")))?;
    assert_eq!(exported, destination.lino_lines());
    Ok(())
}

#[test]
fn normalized_trigger_query_keeps_quoted_names_and_bare_addresses() -> Result<()> {
    let file = NamedTempFile::new()?;
    let mut storage = LinkStorage::new(file.path().to_str().unwrap(), false)?;
    let auto = QueryProcessor::new(false).with_auto_create_missing_references(true);
    let parsed = link_cli::PersistentTransformationQuery::parse(
        "() ((a: a a) (p: a '0') (q: a '1') (r: a 1))",
    )?;
    auto.process_query(&mut storage, &parsed.query())?;
    assert!(storage.get_by_name("0").is_some());
    assert!(storage.get_by_name("1").is_some());
    let q = storage.get_by_name("q").unwrap();
    assert_eq!(
        storage.get(q).unwrap().target,
        storage.get_by_name("1").unwrap()
    );
    let r = storage.get_by_name("r").unwrap();
    assert_eq!(storage.get(r).unwrap().target, 1);
    Ok(())
}

#[test]
fn trigger_normalization_preserves_existing_named_query_text() -> Result<()> {
    let condition = "((p: 'source name' \"target's name\"))";
    let substitution = "((p: 'source name' \"next's name\"))";
    let parsed =
        link_cli::PersistentTransformationQuery::parse(&format!("{condition} {substitution}"))?;
    // Stored trigger identity compares this text, including when removing one.
    assert_eq!(condition, parsed.condition);
    assert_eq!(substitution, parsed.substitution);
    Ok(())
}

#[test]
fn public_validator_preserves_literal_quotes_in_parsed_names() -> Result<()> {
    let file = NamedTempFile::new()?;
    let mut storage = LinkStorage::new(file.path().to_str().unwrap(), false)?;
    let name = "\"one\"";
    storage.get_or_create_named(name);
    // Public callers supply already-decoded identifiers, without the query
    // parser's internal envelopes. Quote characters here belong to the name.
    let reference = LinoLink::new(Some(name.to_string()));
    LinkReferenceValidator::new(false, false).validate_links_exist_or_will_be_created(
        &mut storage,
        &[reference],
        &[],
    )?;
    assert_eq!(storage.get_by_name("one"), None);
    Ok(())
}
