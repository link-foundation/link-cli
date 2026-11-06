use super::*;

#[test]
fn substitution_updates_the_to_part_in_place() -> Result<()> {
    let mut triplets = named_people()?;
    let before = named(&mut triplets, ["r", "knows", "alice", "bob"]);
    let after = named(&mut triplets, ["r", "knows", "alice", "carol"]);
    // The query of the issue: ((i: T f t)) ((i: T f t2)), by address.
    let text = format!(
        "((({}: {} {} {})) (({}: {} {} {})))",
        before.index,
        before.type_id,
        before.from,
        before.to,
        after.index,
        after.type_id,
        after.from,
        after.to
    );
    let changes = query(&mut triplets, &text)?;
    assert_eq!(
        vec![TripletChange {
            before: Some(before),
            after: Some(after)
        }],
        changes
    );
    assert_eq!(Some(after), triplets.get_triplet(before.index));
    // And by name, with a variable.
    let changes = query(
        &mut triplets,
        "((($r: knows alice carol)) (($r: knows alice bob)))",
    )?;
    assert_eq!(Some(before), changes[0].after);
    Ok(())
}

#[test]
fn substitution_reads_creates_and_deletes() -> Result<()> {
    let mut triplets = named_people()?;
    let r = named(&mut triplets, ["r", "knows", "alice", "bob"]);
    // A substitution that repeats the restriction only reads.
    let read = query(&mut triplets, "((($i: knows $f $t)) (($i: knows $f $t)))")?;
    assert_eq!(
        vec![TripletChange {
            before: Some(r),
            after: Some(r)
        }],
        read
    );
    let people_read = query(&mut triplets, "((($p: Person 0 0)) (($p: Person 0 0)))")?;
    assert_eq!(3, people_read.len());

    // A create for every match, with the variable of the match.
    let created = query(
        &mut triplets,
        "((($p: Person 0 0)) (($p: Person 0 0) (knows $p bob)))",
    )?;
    let new: Vec<Triplet> = created
        .iter()
        .filter(|change| change.before.is_none())
        .map(|change| change.after.unwrap())
        .collect();
    assert_eq!(3, new.len());
    let bob = id(&mut triplets, "bob");
    assert_eq!(4, triplets.count_triplets(TripletQuery::by_to(bob)));

    // No restriction: a create; a new name is a new triplet.
    let changes = query(
        &mut triplets,
        "(() ((likes: Type 0 0) (likes carol alice)))",
    )?;
    assert_eq!(2, changes.len());
    assert!(changes.iter().all(|change| change.before.is_none()));
    let likes = id(&mut triplets, "likes");
    assert_eq!(1, triplets.count_triplets(TripletQuery::by_type(likes)));

    // No substitution: a delete, with whatever used the deleted triplet.
    let deleted = query(&mut triplets, "((($i: knows $p bob)) ())")?;
    assert_eq!(4, deleted.len());
    assert!(deleted.iter().all(|change| change.after.is_none()));
    assert_eq!(0, triplets.count_triplets(TripletQuery::by_to(bob)));
    assert!(!triplets.is_triplet(r.index));
    Ok(())
}

#[test]
fn substitution_changes_the_type_part() -> Result<()> {
    let mut triplets = named_people()?;
    query(&mut triplets, "(() ((likes: Type 0 0)))")?;
    let changes = query(&mut triplets, "((($i: knows $f $t)) (($i: likes $f $t)))")?;
    let after = named(&mut triplets, ["r", "likes", "alice", "bob"]);
    assert_eq!(Some(after), changes[0].after);
    // `*` keeps a part, and the parts may swap.
    let changes = query(&mut triplets, "((($i: likes $f $t)) (($i: * $t $f)))")?;
    assert_eq!(
        Some(named(&mut triplets, ["r", "likes", "bob", "alice"])),
        changes[0].after
    );
    // A conjunction: who is liked by someone of type Person.
    let changes = query(
        &mut triplets,
        "((($i: likes $f $t) ($f: Person 0 0)) (($i: likes $f $t) ($f: Person 0 0)))",
    )?;
    assert_eq!(2, changes.len());
    Ok(())
}

#[test]
fn substitution_refuses_unknown_references() -> Result<()> {
    let mut triplets = named_people()?;
    let before = triplets_to_lino(&mut triplets)?;
    for text in [
        "(() ((nobody alice bob)))",
        "(() ((knows $x bob)))",
        "(() ((knows * bob)))",
        "(() ((*: knows alice bob)))",
        "(() ((knows alice)))",
        "(() ((0 alice bob)))",
        "((($i: knows $f $t)) (($i: knows $f $nobody)))",
    ] {
        assert!(
            process_triplets_query(&mut triplets, text).is_err(),
            "{text}"
        );
    }
    assert_eq!(before, triplets_to_lino(&mut triplets)?);
    // A restriction by an unknown name matches nothing.
    assert!(query(&mut triplets, "((($i: nobody $f $t)) ())")?.is_empty());
    Ok(())
}

/// The triplets are plain doublets, so a file-backed store keeps them across
/// a reopen and the named markers are found again.
#[test]
fn keeps_triplets_in_a_named_file_store() -> Result<()> {
    let file = NamedTempFile::new()?;
    let text = {
        let mut triplets = TripletsOverDoublets::with_named_markers(NamedTypesDecorator::new(
            file.path(),
            false,
        )?)?;
        import_triplets_lino(&mut triplets, PEOPLE)?;
        triplets.save()?;
        triplets_to_lino(&mut triplets)?
    };
    let mut triplets =
        TripletsOverDoublets::with_named_markers(NamedTypesDecorator::new(file.path(), false)?)?;
    assert_eq!(text, triplets_to_lino(&mut triplets)?);
    assert_eq!(PEOPLE, text);
    Ok(())
}
