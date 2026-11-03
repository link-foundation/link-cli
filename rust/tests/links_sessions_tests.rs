//! Sessions of a links server, each with its subject (issue #149): the
//! permission cases of `server.test.js` of `@link-foundation/ui-links`; the
//! C# twin is
//! `csharp/Foundation.Data.Doublets.Cli.Tests/Protocol/LinksSessionsTests.cs`,
//! case for case.

mod common;
#[path = "common/permissions_world.rs"]
mod world;

use std::collections::HashMap;

use anyhow::Result;
use common::{protocols, RunningServer};
use link_cli::protocol::{LinksClient, LinksSessions, ServerOptions};
use link_cli::{Link, LinkStorage, LinksTokenTable, NamedTypeLinks};
use world::{all, World};

fn name(store: &mut dyn NamedTypeLinks, link: u32) -> Option<String> {
    store.get_name(link).unwrap()
}

/// The address the next link gets.
fn next(store: &mut dyn NamedTypeLinks) -> u32 {
    store.all_links().len() as u32 + 1
}

/// How many links `subject` sees.
fn seen_by(w: &mut World, subject: u32) -> usize {
    w.as_(subject).all_links().len()
}

fn names_of(store: &mut dyn NamedTypeLinks, links: &[Link]) -> HashMap<u32, Option<String>> {
    links
        .iter()
        .map(|link| (link.index, name(store, link.index)))
        .collect()
}

#[test]
fn authenticates_with_the_callback() {
    let mut sessions = LinksSessions::new(LinkStorage::in_memory(false))
        .with_authenticator(|_, token| Ok((token == "secret").then_some(7)));
    let writer = sessions.open(None);
    assert_eq!(
        "(error: 'Forbidden: authentication failed')",
        sessions.request(writer, "(auth: wrong)")
    );
    assert_eq!(Some(0), sessions.subject(writer));
    assert_eq!("(subject: 7)", sessions.request(writer, "(auth: secret)"));
    assert_eq!(Some(7), sessions.subject(writer));
    assert_eq!("() ((1: 1 1))", sessions.request(writer, "() ((1 1))"));
    assert_eq!(vec![writer], sessions.sessions());
    sessions.close(writer);
    assert!(sessions.sessions().is_empty());
}

#[test]
fn refuses_authentication_without_an_authenticator() {
    let mut sessions = LinksSessions::new(LinkStorage::in_memory(false));
    let session = sessions.open(None);
    assert_eq!(
        "(error: 'Forbidden: authentication failed')",
        sessions.request(session, "(auth: secret)")
    );
}

#[test]
fn serves_each_subject_what_its_permissions_allow() -> Result<()> {
    let mut w = World::new(false)?;
    let (t, v) = (w.t.clone(), w.v.clone());
    let seen = seen_by(&mut w, v["alice"]);
    let mut sessions = LinksSessions::with_permissions(&mut w.links, &mut w.policy);
    let alice = sessions.open(Some(v["alice"]));
    let boss = sessions.open(Some(v["boss"]));
    let (secret, note) = (v["secret"], t["Note"]);
    assert_eq!(
        format!("(count: {seen})"),
        sessions.request(alice, "(count: ())")
    );
    assert_eq!(
        format!("(error: 'Link not found: {secret}')"),
        sessions.request(alice, &format!("(update: ({secret} {note} {secret}))"))
    );
    // set-name answers where the names store keeps the name.
    assert!(sessions
        .request(alice, &format!("(set-name: ({} 'mine'))", v["note"]))
        .starts_with("(link: "));
    assert_eq!(
        Some("mine".to_string()),
        name(sessions.store_mut(), v["note"])
    );
    assert_eq!("", sessions.request(alice, "(get-by-name: 'secret')"));
    assert_eq!(
        "",
        sessions.request(alice, &format!("(get-name: {secret})"))
    );
    assert_eq!("", sessions.request(alice, "((secret: * *)) ()"));
    assert_eq!(
        "(name: secret)",
        sessions.request(boss, &format!("(get-name: {secret})"))
    );
    assert_eq!(
        format!("(link: {secret})"),
        sessions.request(boss, "(get-by-name: 'secret')")
    );
    assert_eq!(
        format!(
            "(({secret}: {} {secret})) (({secret}: {note} {secret}))",
            t["Secret"]
        ),
        sessions.request(boss, &format!("(update: ({secret} {note} {secret}))"))
    );
    assert_eq!(
        format!("(count: {})", seen + 1),
        sessions.request(alice, "(count: ())")
    );
    let guest = sessions.open(None);
    assert_eq!(
        "(error: 'Forbidden: create')",
        sessions.request(guest, "(create: (0 0))")
    );
    Ok(())
}

#[test]
fn lets_a_subject_type_the_links_it_creates_as_types_it_may_not_read() -> Result<()> {
    let mut w = World::new(false)?;
    let (t, v) = (w.t.clone(), w.v.clone());
    let n = next(&mut w.links);
    let mut sessions = LinksSessions::with_permissions(&mut w.links, &mut w.policy);
    let alice = sessions.open(Some(v["alice"]));
    let note = t["Note"];
    // alice may write notes, but may not read the link of the type Note.
    assert_eq!("", sessions.request(alice, &format!("(each: ({note}))")));
    assert_eq!(
        format!("() (({n}: 0 0))"),
        sessions.request(alice, "(create: (0 0))")
    );
    assert_eq!(
        format!("(({n}: 0 0)) (({n}: {note} {n}))"),
        sessions.request(alice, &format!("(update: ({n} {note} {n}))"))
    );
    // (n: Note n) is there already: a store of unique doublets would merge a
    // second one into it, so the next note points to another link.
    let other = v["alice"];
    assert_eq!(
        format!("() (({}: {note} {other}))", n + 1),
        sessions.request(alice, &format!("(create: ({note} {other}))"))
    );
    // The names of types alice cannot read resolve all the same.
    assert_eq!(
        format!("() (({}: Note {}))", n + 2, n + 1),
        sessions.request(alice, &format!("() ((Note {}))", n + 1))
    );
    Ok(())
}

#[test]
fn undoes_a_failed_request_and_reports_its_own_error() -> Result<()> {
    let mut w = World::new(false)?;
    let (t, v) = (w.t.clone(), w.v.clone());
    let n = next(&mut w.links);
    let mut sessions = LinksSessions::with_permissions(&mut w.links, &mut w.policy);
    let alice = sessions.open(Some(v["alice"]));
    let note = t["Note"];
    sessions.request(alice, "(create: (0 0))");
    // The update is allowed before, then refused as it would be after:
    // undoing it needs no permission, and the error is the one of the request.
    // (A store of doublets takes a missing target, so ui-links' 9999 does not fail here.)
    assert_eq!(
        format!("(error: 'Forbidden: update {n}')"),
        sessions.request(alice, &format!("(update: ({n} {} {n}))", t["Secret"]))
    );
    assert_eq!(
        format!("({n}: 0 0)"),
        sessions.request(alice, &format!("(each: ({n}))"))
    );
    assert_eq!(
        format!("(({n}: 0 0)) (({n}: {note} {n}))"),
        sessions.request(alice, &format!("(update: ({n} {note} {n}))"))
    );
    Ok(())
}

#[test]
fn undoes_every_part_of_a_failed_substitution_including_hidden_ones() -> Result<()> {
    let mut w = World::new(false)?;
    let (t, v) = (w.t.clone(), w.v.clone());
    let mut sessions = LinksSessions::with_permissions(&mut w.links, &mut w.policy);
    let alice = sessions.open(Some(v["alice"]));
    let (note, secret_type) = (t["Note"], t["Secret"]);
    let mut before = all(sessions.store_mut());
    let names = names_of(sessions.store_mut(), &before);
    // The first change is allowed, the second is not: neither stays.
    let reply = sessions.request(
        alice,
        &format!("() (({note} {note}) ({secret_type} {secret_type}))"),
    );
    assert!(reply.starts_with("(error: 'Forbidden: "), "{reply}");
    assert_eq!(before, all(sessions.store_mut()));
    assert_eq!(names, names_of(sessions.store_mut(), &before));
    // Deleting a note deletes the links that use it and its name, writes
    // of the store alice does not ask for; the request fails at its
    // forbidden update after the delete, and all of it is undone.
    let k = next(sessions.store_mut());
    let (note_link, shared, shared_type) = (v["note"], v["shared"], t["Shared"]);
    assert_eq!(
        format!("() (({k}: {shared_type} {note_link}))"),
        sessions.request(alice, &format!("(create: ({shared_type} {note_link}))"))
    );
    before = all(sessions.store_mut());
    let request =
        format!("(({note_link}: * *) ({shared}: * *)) (({shared}: {secret_type} {shared}))");
    assert_eq!(
        format!("(error: 'Forbidden: update {shared}')"),
        sessions.request(alice, &request)
    );
    assert_eq!(before, all(sessions.store_mut()));
    assert_eq!(
        Some("note".to_string()),
        name(sessions.store_mut(), note_link)
    );
    assert_eq!(
        Some("shared".to_string()),
        name(sessions.store_mut(), shared)
    );
    // Allowed, the same request deletes both and the name.
    let boss = sessions.open(Some(v["boss"]));
    sessions.request(boss, &request);
    assert!(!sessions.store_mut().exists(note_link));
    assert!(!sessions.store_mut().exists(k));
    assert_eq!(None, name(sessions.store_mut(), note_link));
    Ok(())
}

#[test]
fn does_not_give_a_subject_a_link_made_where_its_own_was_deleted() -> Result<()> {
    let mut w = World::new(false)?;
    let v = w.v.clone();
    let n = next(&mut w.links);
    let mut sessions = LinksSessions::with_permissions(&mut w.links, &mut w.policy);
    let alice = sessions.open(Some(v["alice"]));
    let boss = sessions.open(Some(v["boss"]));
    sessions.request(alice, "(create: (0 0))");
    assert_eq!(
        format!("({n}: 0 0)"),
        sessions.request(alice, &format!("(each: ({n}))"))
    );
    sessions.request(boss, &format!("(delete: {n})"));
    assert_eq!(
        format!("() (({n}: 0 0))"),
        sessions.request(boss, "(create: (0 0))")
    );
    assert_eq!("", sessions.request(alice, &format!("(each: ({n}))")));
    assert_eq!(
        format!("(error: 'Link not found: {n}')"),
        sessions.request(alice, &format!("(update: ({n} 0 0))"))
    );
    Ok(())
}

#[test]
fn serves_rules_in_a_store_of_their_own_alike() -> Result<()> {
    let mut w = World::new(true)?;
    let v = w.v.clone();
    let expected = seen_by(&mut w, v["alice"]);
    let mut sessions = LinksSessions::with_permissions(&mut w.links, &mut w.policy);
    let alice = sessions.open(Some(v["alice"]));
    assert_eq!(
        format!("(count: {expected})"),
        sessions.request(alice, "(count: ())")
    );
    assert_eq!("", sessions.request(alice, "((secret: * *)) ()"));
    // set-name answers where the names store keeps the name.
    assert!(sessions
        .request(alice, &format!("(set-name: ({} 'mine'))", v["note"]))
        .starts_with("(link: "));
    assert_eq!(
        Some("mine".to_string()),
        name(sessions.store_mut(), v["note"])
    );
    Ok(())
}

#[test]
fn without_permissions_every_session_sees_the_whole_store() -> Result<()> {
    let mut w = World::new(false)?;
    let v = w.v.clone();
    let count = w.links.all_links().len();
    let mut sessions = LinksSessions::new(&mut w.links);
    let alice = sessions.open(Some(v["alice"]));
    assert_eq!(
        format!("(count: {count})"),
        sessions.request(alice, "(count: ())")
    );
    assert_eq!(
        format!("(link: {})", v["secret"]),
        sessions.request(alice, "(get-by-name: 'secret')")
    );
    Ok(())
}

#[test]
fn authenticates_with_a_token_table_in_links() -> Result<()> {
    let mut w = World::new(false)?;
    let v = w.v.clone();
    let seen = seen_by(&mut w, v["alice"]);
    let mut table = LinksTokenTable::new(&mut w.links);
    assert_eq!(None, table.authenticate("alice-token")?);
    let link = table.grant("alice-token", v["alice"])?;
    table.grant("alice-second-token", v["alice"])?;
    table.grant("boss-token", v["boss"])?;
    assert_eq!(Some(v["alice"]), table.authenticate("alice-token")?);
    assert_eq!(Some(v["alice"]), table.authenticate("alice-second-token")?);
    assert_eq!(Some(v["boss"]), table.authenticate("boss-token")?);
    assert_eq!(None, table.authenticate("wrong")?);
    // The store holds a hash of the token, never the token itself.
    let stored = name(table.store_mut(), link).unwrap();
    assert_eq!(
        LinksTokenTable::<LinkStorage>::name_of("alice-token"),
        stored
    );
    assert!(!stored.contains("alice-token"));
    assert_eq!(link, table.grant("alice-token", v["boss"])?);
    assert_eq!(Some(v["boss"]), table.authenticate("alice-token")?);

    let mut sessions = LinksSessions::with_permissions(&mut w.links, &mut w.policy)
        .with_authenticator(|store, token| LinksTokenTable::new(store).authenticate(token));
    let session = sessions.open(None);
    assert_eq!(
        "(error: 'Forbidden: create')",
        sessions.request(session, "(create: (0 0))")
    );
    assert_eq!(
        format!("(subject: {})", v["alice"]),
        sessions.request(session, "(auth: alice-second-token)")
    );
    assert_eq!(
        format!("(count: {seen})"),
        sessions.request(session, "(count: ())")
    );
    let mut table = LinksTokenTable::new(sessions.store_mut());
    assert!(table.revoke("alice-second-token")?);
    assert!(!table.revoke("alice-second-token")?);
    assert_eq!(None, table.authenticate("alice-second-token")?);
    assert_eq!(
        "(error: 'Forbidden: authentication failed')",
        sessions.request(session, "(auth: alice-second-token)")
    );
    Ok(())
}

#[test]
fn authenticates_over_tcp() {
    let server = RunningServer::start_with(ServerOptions::default(), |server, path| {
        let storage = LinkStorage::new(path, false).unwrap();
        let mut sessions = LinksSessions::new(storage)
            .with_authenticator(|_, token| Ok((token == "secret").then_some(7)));
        server.serve_sessions(&mut sessions).unwrap();
    });
    for protocol in protocols() {
        let mut client: LinksClient = server.client(protocol);
        assert_eq!(7, client.authenticate("secret").unwrap());
        let error = client.authenticate("wrong").unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Forbidden: authentication failed"),
            "{error}"
        );
    }
}
