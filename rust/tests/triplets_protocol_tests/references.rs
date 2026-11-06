use super::*;

#[test]
fn a_part_it_may_not_refer_to_answers_as_a_missing_one() {
    const MISSING: u32 = 9999;
    let (server, t, v) = permitted_server();
    let (note_type, classified) = (t["Note"], v["secret"]);
    let mut alice = authenticated(&server, "alice", v["alice"]);
    let own = alice
        .query(&format!("(create: ({note_type} 0 0))"))
        .unwrap();
    let own = parse_triplet_changes(&own).unwrap()[0].after.unwrap().index;

    let requests: [&dyn Fn(u32) -> String; 4] = [
        &|part| format!("(create: ({note_type} {part} 0))"),
        &|part| format!("(create: ({note_type} 0 {part}))"),
        &|part| format!("(update: ({own} {note_type} {part} 0))"),
        &|part| format!("() (({part}: {note_type} 0 0))"),
    ];
    for request in requests {
        let hidden = alice.query_text(&request(classified)).unwrap_err();
        let missing = alice.query_text(&request(MISSING)).unwrap_err();
        assert_eq!(
            hidden.to_string().replace(&classified.to_string(), "N"),
            missing.to_string().replace(&MISSING.to_string(), "N"),
        );
    }
    // The write check runs first: a type it may not write is forbidden, hidden or missing.
    for type_id in [t["Secret"], MISSING] {
        let error = alice
            .query_text(&format!("(update: ({own} {type_id} 0 0))"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("Forbidden"), "{type_id}: {error}");
    }
}

#[test]
fn the_right_to_refer_to_a_triplet_does_not_allow_reading_it() {
    let (server, t, v) = permitted_server_with(|w| {
        let (user, unreadable) = (w.t["User"], w.t["Secret"]);
        w.allow(link_cli::PermissionAccess::Reference, user, unreadable)
            .unwrap();
    });
    let (note_type, classified) = (t["Note"], v["secret"]);
    let mut alice = authenticated(&server, "alice", v["alice"]);
    let reply = alice
        .query(&format!("(create: ({note_type} {classified} 0))"))
        .unwrap();
    let created = parse_triplet_changes(&reply).unwrap()[0].after.unwrap();
    assert_eq!(created.from, classified);
    assert_eq!(
        alice
            .query_text(&format!("(each: ({classified}))"))
            .unwrap(),
        ""
    );
    // A missing address stays missing.
    let error = alice
        .query_text(&format!("(create: ({note_type} 9999 0))"))
        .unwrap_err()
        .to_string();
    assert!(error.contains("is not a triplet"), "{error}");
    // Its address stays taken: no point lands on it.
    let error = alice
        .query_text(&format!("() (({classified}: {note_type} 0 0))"))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains(&format!("Link {classified} exists")),
        "{error}"
    );
}

#[test]
fn a_part_a_triplet_holds_may_stay_but_cannot_be_put_back() {
    let (server, t, v) = permitted_server();
    let (note_type, classified, note) = (t["Note"], v["secret"], v["note"]);
    let mut alice = authenticated(&server, "alice", v["alice"]);
    let mut boss = authenticated(&server, "boss", v["boss"]);
    let held = boss
        .query(&format!("(create: ({note_type} {classified} 0))"))
        .unwrap();
    let held = parse_triplet_changes(&held).unwrap()[0]
        .after
        .unwrap()
        .index;

    alice
        .query_text(&format!(
            "(update: ({held} {note_type} {classified} {note}))"
        ))
        .unwrap();
    alice
        .query_text(&format!("(update: ({held} {note_type} 0 {note}))"))
        .unwrap();
    let error = alice
        .query_text(&format!(
            "(update: ({held} {note_type} {classified} {note}))"
        ))
        .unwrap_err()
        .to_string();
    assert!(error.contains("is not a triplet"), "{error}");
}
