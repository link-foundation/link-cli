//! Triplets over the text and binary protocol (issue #152): a server in
//! triplet mode answers with triplets, a server without it answers as it always
//! did, and [`RemoteLinks`] is a drop-in replacement for a local store of
//! triplets.

mod common;
#[path = "common/permissions_world.rs"]
mod world;

use common::{protocols, RunningServer};
use link_cli::protocol::triplets_operations::{
    canonical_triplets, execute_triplets_request, pack_triplets, parse_triplet_changes,
    parse_triplet_notification, parse_triplets, triplet_notification_document,
};
use link_cli::protocol::{
    decode_document, encode_document, format_document, parse_document, ArityRange,
    BinaryLinoOptions, BinaryLinoProtocol, DecodeLimits, LinksClient, LinksPacket, LinksSessions,
    MessageFormat, Reference, RemoteLinks, ServerOptions, TextLinoProtocol, TripletsNotification,
    TripletsOperation,
};
use link_cli::{
    process_triplets_query, NamedTypeLinks, NamedTypesDecorator, Triplet, TripletChange,
    TripletQuery, Triplets, TripletsOverDoublets,
};
use std::io::Write;
use std::net::TcpStream;
use std::sync::mpsc;
use std::time::Duration;
use tempfile::NamedTempFile;

const PUSH_TIMEOUT: Duration = Duration::from_secs(10);
const SILENCE_TIMEOUT: Duration = Duration::from_millis(200);

fn triplet_server() -> RunningServer {
    RunningServer::start(
        ServerOptions {
            triplets: true,
            ..ServerOptions::default()
        },
        true,
    )
}

/// One step of `docs/protocol/triplets.txt`: a request and its reply, or an
/// expected push.
struct Step {
    connection: char,
    is_push: bool,
    message: &'static str,
    expected: String,
}

/// The steps of `docs/protocol/triplets.txt`, which the C# and JavaScript
/// tests replay too.
fn conversation() -> Vec<Step> {
    let starts = |line: &str| {
        let bytes = line.as_bytes();
        bytes.len() > 2
            && matches!(bytes[0], b'A' | b'B')
            && matches!(bytes[1], b'>' | b'<')
            && bytes[2] == b' '
    };
    let lines: Vec<&str> = include_str!("../../docs/protocol/triplets.txt")
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    let mut steps = Vec::new();
    let mut start = 0;
    while start < lines.len() {
        assert!(
            starts(lines[start]),
            "expected a step, found '{}'",
            lines[start]
        );
        let end = (start + 1..lines.len())
            .find(|&line| starts(lines[line]))
            .unwrap_or(lines.len());
        let is_push = lines[start].as_bytes()[1] == b'<';
        let message = &lines[start][3..];
        let mut expected: Vec<&str> = if is_push { vec![message] } else { Vec::new() };
        expected.extend_from_slice(&lines[start + 1..end]);
        steps.push(Step {
            connection: lines[start].chars().next().unwrap(),
            is_push,
            message,
            expected: expected.join("\n"),
        });
        start = end;
    }
    steps
}

#[test]
fn servers_replay_the_shared_triplets_conversation_over_every_protocol() {
    let conversation = conversation();
    assert_eq!(conversation.len(), 48);
    for (first, second) in protocols().into_iter().zip(protocols()) {
        let server = triplet_server();
        let mut a = server.client(first);
        let mut b = server.client(second);
        for step in &conversation {
            let client = if step.connection == 'A' {
                &mut a
            } else {
                &mut b
            };
            if step.is_push {
                let push = client
                    .receive_notification(PUSH_TIMEOUT)
                    .unwrap()
                    .expect("a push");
                assert_eq!(
                    format_document(&canonical_triplets(push)),
                    step.expected,
                    "{}",
                    step.message
                );
                continue;
            }
            let reply = client.query(step.message);
            match step.expected.strip_prefix("! ") {
                Some(message) => {
                    let error = reply.unwrap_err().to_string();
                    assert!(error.ends_with(message), "{}: {error}", step.message);
                }
                None => assert_eq!(
                    (
                        step.message,
                        format_document(&canonical_triplets(reply.unwrap()))
                    ),
                    (step.message, step.expected.clone())
                ),
            }
            assert_eq!(client.pending_notifications(), 0, "{}", step.message);
        }
        assert!(a.receive_notification(SILENCE_TIMEOUT).unwrap().is_none());
        assert!(b.receive_notification(SILENCE_TIMEOUT).unwrap().is_none());
    }
}

#[test]
fn a_server_without_triplet_mode_answers_as_it_always_did() {
    let server = RunningServer::start(ServerOptions::default(), true);
    let mut client = server.client(TextLinoProtocol::new());

    assert_eq!(
        client.query_text("(create: (0 0))").unwrap(),
        "() ((1: 0 0))"
    );
    assert_eq!(client.query_text("(each: (* * *))").unwrap(), "(1: 0 0)");
    for request in [
        "(create: (1 1 1))",
        "(update: (1 1 1 1))",
        "(each: (1 2 3 4))",
    ] {
        assert!(client.query_text(request).is_err(), "{request}");
    }
    // It created no markers either.
    assert_eq!(client.query_text("(count: ())").unwrap(), "(count: 1)");
}

/// A raw connection, to see the packet the server writes.
fn raw_request(stream: &mut TcpStream, request: &str, options: BinaryLinoOptions) -> LinksPacket {
    let document = parse_document(request).unwrap();
    let bytes = encode_document(&document, options)
        .unwrap()
        .to_bytes()
        .unwrap();
    stream.write_all(&bytes).unwrap();
    LinksPacket::read_from(stream, &DecodeLimits::default())
        .unwrap()
        .expect("a reply")
}

#[test]
fn a_client_that_asks_for_two_to_three_gets_each_triplet_as_one_link_of_three_references() {
    let server = triplet_server();
    let mut text = server.client(TextLinoProtocol::new());
    for request in [
        "(create: (0 0 0))",
        "(create: (0 0 0))",
        "(create: (3 0 0))",
        "(create: (3 0 0))",
        "(create: (4 5 6))",
    ] {
        text.query(request).unwrap();
    }
    let options = BinaryLinoOptions {
        external_references: true,
        arity: ArityRange::between(2, 3),
        packed_widths: false,
    };
    let mut stream = TcpStream::connect(server.address).unwrap();

    let reply = raw_request(&mut stream, "(each: (* 4 5 6))", options);

    let (type_id, from, to) = (4, 5, 6);
    assert!(
        reply.links().any(|(_, link)| {
            link == [
                Reference::External(type_id),
                Reference::External(from),
                Reference::External(to),
            ]
        }),
        "{:?}",
        reply.links().collect::<Vec<_>>()
    );
    assert_eq!(
        format_document(&decode_document(&reply, &DecodeLimits::default()).unwrap()),
        "(9: (4 5 6))"
    );
    assert!(reply
        .links()
        .all(|(_, link)| link.len() == 2 || link.len() == 3));

    // Doublets alone keep the form as written, spelled in pairs.
    let doublets = BinaryLinoOptions {
        arity: ArityRange::DOUBLETS,
        ..options
    };
    let reply = raw_request(&mut stream, "(each: (* 4 5 6))", doublets);
    assert!(reply.links().all(|(_, link)| link.len() == 2));
    assert_eq!(
        format_document(&decode_document(&reply, &DecodeLimits::default()).unwrap()),
        "(9: 4 5 6)"
    );
}

#[test]
fn packing_changes_only_the_triplets_of_a_format_that_holds_three_but_not_five() {
    let document = parse_document("((1: 2 3 4)) ((1: 2 3 5))\n(count: 1)\n(name: a)").unwrap();
    let with = |arity| {
        MessageFormat::Binary(BinaryLinoOptions {
            arity,
            ..BinaryLinoOptions::default()
        })
    };

    let packed = pack_triplets(document.clone(), with(ArityRange::between(2, 3)));
    assert_eq!(
        format_document(&packed),
        "((1: (2 3 4))) ((1: (2 3 5)))\n(count: 1)\n(name: a)"
    );
    assert_eq!(canonical_triplets(packed), document);

    for format in [
        MessageFormat::Text,
        with(ArityRange::DOUBLETS),
        with(ArityRange::at_least(1)),
    ] {
        assert_eq!(pack_triplets(document.clone(), format), document);
    }
}

#[test]
fn every_parser_reads_both_forms_of_a_triplet() {
    let triplet = Triplet::new(1, 2, 3, 4);
    for text in ["(1: 2 3 4)", "(1: (2 3 4))"] {
        assert_eq!(
            parse_triplets(&parse_document(text).unwrap()).unwrap(),
            vec![triplet]
        );
    }
    for text in ["((1: 2 3 4)) ()", "((1: (2 3 4))) ()"] {
        assert_eq!(
            parse_triplet_changes(&parse_document(text).unwrap()).unwrap(),
            vec![TripletChange {
                before: Some(triplet),
                after: None,
            }]
        );
    }
    let notification = TripletsNotification {
        subscription: 3,
        changes: vec![TripletChange {
            before: None,
            after: Some(triplet),
        }],
    };
    let document = triplet_notification_document(3, &notification.changes);
    assert_eq!(format_document(&document), "(notify: 3)\n() ((1: 2 3 4))");
    assert_eq!(parse_triplet_notification(&document).unwrap(), notification);
}

#[test]
fn operations_round_trip_through_their_documents() {
    for (operation, text) in [
        (TripletsOperation::Count(vec![]), "(count: ())"),
        (
            TripletsOperation::Each(vec![None, Some(4), None, Some(0)]),
            "(each: (* 4 * 0))",
        ),
        (
            TripletsOperation::Create {
                type_id: 4,
                from: 5,
                to: 6,
            },
            "(create: (4 5 6))",
        ),
        (
            TripletsOperation::Update {
                index: 9,
                type_id: 4,
                from: 5,
                to: 0,
            },
            "(update: (9 4 5 0))",
        ),
        (TripletsOperation::Delete(9), "(delete: 9)"),
        (
            TripletsOperation::Subscribe(vec![None, Some(4)]),
            "(subscribe: (* 4))",
        ),
    ] {
        assert_eq!(format_document(&operation.to_document()), text);
        assert_eq!(
            TripletsOperation::from_document(&parse_document(text).unwrap()).unwrap(),
            Some(operation)
        );
    }
    for other in ["(get-name: 1)", "() ((1 2 3))", "(1: 2 3 4)", ""] {
        assert_eq!(
            TripletsOperation::from_document(&parse_document(other).unwrap()).unwrap(),
            None,
            "{other}"
        );
    }
}

fn local_triplets() -> (NamedTempFile, NamedTypesDecorator) {
    let database = NamedTempFile::new().unwrap();
    let store = NamedTypesDecorator::new(database.path().to_str().unwrap(), false).unwrap();
    (database, store)
}

#[test]
fn execute_triplets_request_answers_like_a_server_in_triplet_mode() {
    let (_database, mut store) = local_triplets();
    let mut triplets = TripletsOverDoublets::with_named_markers(&mut store).unwrap();
    let mut request = |text: &str| {
        format_document(&execute_triplets_request(
            &mut triplets,
            &parse_document(text).unwrap(),
        ))
    };

    assert_eq!(request(""), "");
    assert_eq!(request("(create: (0 0 0))"), "() ((3: 3 0 0))");
    assert_eq!(request("(create: (3 0 0))"), "() ((4: 3 0 0))");
    assert_eq!(request(""), "(3: 3 0 0)\n(4: 3 0 0)");
    assert_eq!(
        request("(subscribe: ())"),
        "(error: 'subscriptions belong to a connection to a links server')"
    );
    assert_eq!(request("(delete: 3)"), "((3: 3 0 0)) ()\n((4: 3 0 0)) ()");
}

/// Every call of [`Triplets`] once, returning what the store answered.
fn exercise(store: &mut dyn Triplets) -> Vec<String> {
    let person = store.create_triplet_point().unwrap();
    let knows = store.create_triplet_point().unwrap();
    let alice = store.create_triplet(person, 0, 0).unwrap();
    let bob = store.create_triplet(person, 0, 0).unwrap();
    let first = store.create_triplet(knows, alice, bob).unwrap();
    let second = store.create_triplet(knows, alice, bob).unwrap();
    store.create_triplet_point_at(77).unwrap();
    let mut answers = vec![
        format!("{person} {knows} {alice} {bob} {first} {second}"),
        format!("{:?}", store.get_triplet(first)),
        format!("{:?}", store.get_triplet(store.triplet_markers().pair)),
        format!("{}", store.is_triplet(first)),
        format!("{}", store.is_triplet(store.triplet_markers().pair)),
        format!("{}", store.count_triplets(TripletQuery::any())),
        format!("{:?}", store.each_triplet(TripletQuery::by_type(knows))),
        format!("{:?}", store.each_triplet(TripletQuery::by_from(alice))),
        format!("{:?}", store.each_triplet(TripletQuery::by_to(bob))),
        format!("{:?}", store.update_triplet(first, knows, bob, alice).ok()),
        format!("{:?}", store.get_triplet(first)),
        format!("{:?}", store.delete_triplet(alice).ok()),
        format!("{:?}", store.each_triplet(TripletQuery::any())),
    ];
    for result in [
        store.create_triplet(99, 0, 0).map(|_| ()),
        store.create_triplet(0, person, person).map(|_| ()),
        store.update_triplet(99, knows, 0, 0).map(|_| ()),
        store.delete_triplet(99).map(|_| ()),
    ] {
        answers.push(result.unwrap_err().to_string());
    }
    answers
}

#[test]
fn every_triplets_call_matches_a_local_store() {
    let (_database, mut store) = local_triplets();
    let mut local = TripletsOverDoublets::with_named_markers(&mut store).unwrap();
    let expected = exercise(&mut local);

    for protocol in protocols() {
        let server = triplet_server();
        let mut remote = RemoteLinks::new(server.client(protocol));
        let answers = exercise(&mut remote);
        assert_eq!(answers.len(), expected.len());
        for (answer, expected) in answers.iter().zip(&expected) {
            assert!(
                answer == expected || answer.ends_with(expected.as_str()),
                "{answer} != {expected}"
            );
        }
    }
}

#[test]
fn the_query_processor_gives_the_same_results_on_a_remote_store() {
    const QUERIES: &[&str] = &[
        "() ((1 2 3))",
        "() ((person: person 0 0))",
        "() ((knows: knows 0 0))",
        "() ((alice: person 0 0))",
        "() ((bob: person 0 0))",
        "() ((knows alice bob))",
        "((($i: knows $f $t))) (($i: knows $t $f))",
        "(($i: person 0 0)) ()",
    ];
    fn run(store: &mut dyn Triplets) -> Vec<String> {
        QUERIES
            .iter()
            .map(|query| format!("{query} {:?}", process_triplets_query(store, query)))
            .map(|answer| answer.replace("Remote error: ", ""))
            .collect()
    }
    let (_database, mut store) = local_triplets();
    let mut local = TripletsOverDoublets::with_named_markers(&mut store).unwrap();
    let expected = run(&mut local);

    let server = triplet_server();
    let mut remote = RemoteLinks::new(server.client(TextLinoProtocol::new()));
    assert_eq!(run(&mut remote), expected);
    assert_eq!(
        remote.each_triplet(TripletQuery::any()),
        local.each_triplet(TripletQuery::any())
    );
}

#[test]
fn names_stay_available_through_remote_links() {
    let server = triplet_server();
    let mut remote = RemoteLinks::new(server.client(TextLinoProtocol::new()));
    let point = remote.create_triplet_point().unwrap();

    remote.set_name(point, "person").unwrap();

    assert_eq!(remote.get_by_name("person").unwrap(), Some(point));
    assert_eq!(remote.get_name(point).unwrap().as_deref(), Some("person"));
    assert!(remote.exists(point));
    assert!(!remote.exists(999));
    assert_eq!(remote.triplet_markers().pair, 1);
    assert_eq!(remote.triplet_markers().nothing, 2);
}

#[test]
fn a_remote_subscriber_is_told_about_another_connections_triplets() {
    for (first, second) in protocols().into_iter().zip(protocols()) {
        let server = triplet_server();
        let subscriber = RemoteLinks::new(server.client(first));
        let mut writer = RemoteLinks::new(server.client(second));
        let person = writer.create_triplet_point().unwrap();
        let knows = writer.create_triplet_point().unwrap();

        let subscription = subscriber
            .subscribe_triplets(&[None, Some(person)])
            .unwrap();
        assert_eq!(subscription, 1);
        let alice = writer.create_triplet(person, 0, 0).unwrap();
        // A triplet of another type is not pushed.
        writer.create_triplet(knows, alice, alice).unwrap();

        assert_eq!(
            subscriber
                .receive_triplet_notification(PUSH_TIMEOUT)
                .unwrap(),
            Some(TripletsNotification {
                subscription,
                changes: vec![TripletChange {
                    before: None,
                    after: Some(Triplet::new(alice, person, 0, 0)),
                }],
            })
        );
        assert_eq!(
            subscriber
                .receive_triplet_notification(SILENCE_TIMEOUT)
                .unwrap(),
            None
        );

        subscriber.unsubscribe(subscription).unwrap();
        writer.create_triplet(person, 0, 0).unwrap();
        assert_eq!(
            subscriber
                .receive_triplet_notification(SILENCE_TIMEOUT)
                .unwrap(),
            None
        );
    }
}

/// A server in triplet mode over the world of the permission tests, whose
/// values are triplets: `(note: Note note)` reads `(note: Note 0 0)`.
fn permitted_server() -> (
    RunningServer,
    std::collections::HashMap<&'static str, u32>,
    std::collections::HashMap<&'static str, u32>,
) {
    let (sent, received) = mpsc::channel();
    let server = RunningServer::start_serving(
        ServerOptions {
            triplets: true,
            ..ServerOptions::default()
        },
        move |server, _| {
            let mut w = world::World::new(false).unwrap();
            let v = w.v.clone();
            sent.send((w.t.clone(), w.v.clone())).unwrap();
            let mut sessions = LinksSessions::with_permissions(&mut w.links, &mut w.policy)
                .with_authenticator(move |_, token| Ok(v.get(token).copied()));
            server.serve_sessions(&mut sessions).unwrap();
        },
    );
    let (t, v) = received.recv().unwrap();
    (server, t, v)
}

fn authenticated(server: &RunningServer, name: &str, id: u32) -> LinksClient {
    let mut client = server.client(TextLinoProtocol::new());
    assert_eq!(client.authenticate(name).unwrap(), id);
    client
}

#[test]
fn a_subject_reads_and_writes_only_the_triplets_it_may() {
    let (server, t, v) = permitted_server();
    let (alice, boss, note, shared, secret) =
        (v["alice"], v["boss"], v["note"], v["shared"], v["secret"]);
    let mut alice_client = authenticated(&server, "alice", alice);
    let mut boss_client = authenticated(&server, "boss", boss);

    // boss writes anything, so reads anything.
    assert_eq!(
        boss_client
            .query_text(&format!("(each: ({secret}))"))
            .unwrap(),
        format!("({secret}: {} 0 0)", t["Secret"])
    );
    // alice reads the notes she may write and what is public; a secret is not
    // there at all.
    assert_eq!(
        alice_client
            .query_text(&format!("(each: ({secret}))"))
            .unwrap(),
        ""
    );
    assert_eq!(
        alice_client
            .query_text(&format!("(each: (* {}))", t["Secret"]))
            .unwrap(),
        ""
    );
    assert_eq!(
        alice_client
            .query_text(&format!("(each: ({note}))"))
            .unwrap(),
        format!("({note}: {} 0 0)", t["Note"])
    );
    assert_eq!(
        alice_client
            .query_text(&format!("(each: ({shared}))"))
            .unwrap(),
        format!("({shared}: {} 0 0)", t["Shared"])
    );
    assert_eq!(
        alice_client
            .query_text(&format!("(count: ({secret}))"))
            .unwrap(),
        "(count: 0)"
    );

    // alice may write notes: creating one, and turning it to point at another.
    let created = alice_client
        .query(&format!("(create: ({} {note} {shared}))", t["Note"]))
        .unwrap();
    let created = parse_triplet_changes(&created).unwrap()[0].after.unwrap();
    assert_eq!(created.from, note);
    assert_eq!(
        alice_client
            .query_text(&format!(
                "(update: ({} {} {shared} {note}))",
                created.index, t["Note"]
            ))
            .unwrap(),
        format!(
            "(({i}: {n} {note} {shared})) (({i}: {n} {shared} {note}))",
            i = created.index,
            n = t["Note"]
        )
    );

    // She may not make one of the secrets, nor make anything secret.
    for forbidden in [
        format!("(create: ({} 0 0))", t["Secret"]),
        format!("(update: ({} {} 0 0))", created.index, t["Secret"]),
    ] {
        let error = alice_client.query_text(&forbidden).unwrap_err().to_string();
        assert!(error.contains("Forbidden"), "{forbidden}: {error}");
    }
    // What she cannot read is not a triplet to her.
    for hidden in [
        format!("(delete: {secret})"),
        format!("(update: ({secret} {} 0 0))", t["Note"]),
        format!("(create: ({} {secret} 0))", t["Note"]),
    ] {
        let error = alice_client.query_text(&hidden).unwrap_err().to_string();
        assert!(error.contains("is not a triplet"), "{hidden}: {error}");
    }

    // A triplet of boss that uses the note she may delete hides in the
    // cascade: the whole delete is refused, and nothing changes.
    let hidden = boss_client
        .query(&format!("(create: ({} {note} {note}))", t["Secret"]))
        .unwrap();
    let hidden = parse_triplet_changes(&hidden).unwrap()[0].after.unwrap();
    let error = alice_client
        .query_text(&format!("(delete: {note})"))
        .unwrap_err()
        .to_string();
    assert!(error.contains("Forbidden"), "{error}");
    assert_eq!(
        boss_client
            .query_text(&format!("(each: ({}))", hidden.index))
            .unwrap(),
        format!("({}: {} {note} {note})", hidden.index, t["Secret"])
    );
    assert_eq!(
        boss_client
            .query_text(&format!("(each: ({note}))"))
            .unwrap(),
        format!("({note}: {} 0 0)", t["Note"])
    );

    // Boss may.
    assert!(boss_client
        .query_text(&format!("(delete: {note})"))
        .unwrap()
        .starts_with(&format!("(({note}: {} 0 0)) ()", t["Note"])));
}

#[test]
fn a_subject_keeps_the_points_it_created_and_a_guest_writes_nothing() {
    let (server, t, v) = permitted_server();
    let mut alice = authenticated(&server, "alice", v["alice"]);
    let mut boss = authenticated(&server, "boss", v["boss"]);
    let note = v["note"];

    // A point has no type to be judged by: it is the subject's own, until it
    // gets a type.
    let created = alice.query("(create: (0 0 0))").unwrap();
    let point = parse_triplet_changes(&created).unwrap()[0].after.unwrap();
    assert_eq!(point.type_id, point.index);
    let index = point.index;
    assert_eq!(
        alice.query_text(&format!("(each: ({index}))")).unwrap(),
        format!("({index}: {index} 0 0)")
    );
    let other = boss.query_text(&format!("(update: ({index} {index} {note} 0))"));
    assert!(other.is_ok(), "{other:?}");
    assert_eq!(alice.query_text(&format!("(each: ({index}))")).unwrap(), "");

    // A substitution query reserves the point at the address it names, and
    // reads what it refers to: the type she may write yet not read is not there
    // for it, unlike for a create.
    assert_eq!(
        alice.query_text("() ((900: 900 0 0))").unwrap(),
        "() ((900: 900 0 0))"
    );
    let error = alice
        .query_text(&format!("() ((901: {} 0 0))", t["Note"]))
        .unwrap_err()
        .to_string();
    assert!(error.contains("is not a triplet"), "{error}");
    assert_eq!(
        alice.query_text("(delete: 900)").unwrap(),
        "((900: 900 0 0)) ()"
    );

    let mut guest = server.client(TextLinoProtocol::new());
    for request in [
        "(create: (0 0 0))".to_string(),
        format!("(create: ({} 0 0))", t["Note"]),
        "() ((901: 901 0 0))".to_string(),
    ] {
        let error = guest.query_text(&request).unwrap_err().to_string();
        assert!(error.contains("Forbidden"), "{request}: {error}");
    }
    assert_eq!(guest.query_text("(count: ())").unwrap(), "(count: 0)");
}

#[test]
fn a_subscriber_is_pushed_only_the_triplets_its_subject_may_read() {
    let (server, t, v) = permitted_server();
    let (alice, boss, note, secret) = (v["alice"], v["boss"], v["note"], v["secret"]);
    let subscriber = RemoteLinks::new(authenticated(&server, "alice", alice));
    let mut writer = authenticated(&server, "boss", boss);
    subscriber.subscribe_triplets(&[]).unwrap();

    // A change she cannot see is not pushed to her at all...
    writer
        .query(&format!("(update: ({secret} {} {alice} 0))", t["Secret"]))
        .unwrap();
    // ...while one to a note she may read is.
    writer
        .query(&format!("(update: ({note} {} {alice} 0))", t["Note"]))
        .unwrap();

    assert_eq!(
        subscriber
            .receive_triplet_notification(PUSH_TIMEOUT)
            .unwrap(),
        Some(TripletsNotification {
            subscription: 1,
            changes: vec![TripletChange {
                before: Some(Triplet::new(note, t["Note"], 0, 0)),
                after: Some(Triplet::new(note, t["Note"], alice, 0)),
            }],
        })
    );
    assert_eq!(
        subscriber
            .receive_triplet_notification(SILENCE_TIMEOUT)
            .unwrap(),
        None
    );
}

#[test]
fn a_binary_client_of_two_to_three_gets_packed_pushes_too() {
    let server = triplet_server();
    let options = BinaryLinoOptions {
        arity: ArityRange::between(2, 3),
        ..BinaryLinoOptions::default()
    };
    let mut subscriber = server.client(BinaryLinoProtocol::with_options(options));
    let mut writer = server.client(TextLinoProtocol::new());
    subscriber.query("(subscribe: ())").unwrap();
    writer.query("(create: (0 0 0))").unwrap();
    let push = subscriber
        .receive_notification(PUSH_TIMEOUT)
        .unwrap()
        .expect("a push");
    assert_eq!(format_document(&push), "(notify: 1)\n() ((3: (3 0 0)))");
    assert_eq!(
        parse_triplet_notification(&push).unwrap().changes,
        vec![TripletChange {
            before: None,
            after: Some(Triplet::new(3, 3, 0, 0)),
        }]
    );
}

#[test]
fn triplet_mode_keeps_an_existing_store_of_doublets_readable() {
    // The server only adds the markers; a store that had triplets keeps them.
    let database = NamedTempFile::new().unwrap();
    let path = database.path().to_str().unwrap().to_string();
    let existing = {
        let mut store = NamedTypesDecorator::new(&path, false).unwrap();
        let mut triplets = TripletsOverDoublets::with_named_markers(&mut store).unwrap();
        let point = triplets.create_triplet_point().unwrap();
        let index = triplets.create_triplet(point, 0, 0).unwrap();
        NamedTypeLinks::save(&mut triplets).unwrap();
        Triplet::new(index, point, 0, 0)
    };
    let server = RunningServer::start_serving(
        ServerOptions {
            triplets: true,
            ..ServerOptions::default()
        },
        move |server, _| {
            let mut store = NamedTypesDecorator::new(&path, false).unwrap();
            server.serve(&mut store).unwrap();
        },
    );
    let remote = RemoteLinks::new(server.client(TextLinoProtocol::new()));
    assert!(remote
        .triplets_matching(&[Some(existing.index)])
        .unwrap()
        .contains(&existing));
}
