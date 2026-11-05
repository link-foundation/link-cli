//! `(subscribe: …)`, `(notify: …)` and `(unsubscribe: …)`: a connection is
//! told about the changes other connections make to the links it subscribed
//! to, and only those.

mod common;
#[path = "common/permissions_world.rs"]
mod world;

use common::{protocols, RunningServer};
use link_cli::protocol::links_operations::{
    is_notification, notification_document, parse_notification, parse_subscription,
    subscription_document,
};
use link_cli::protocol::{
    execute_request, format_document, parse_document, LinksNotification, LinksOperation,
    LinksSessions, ProtocolError, RemoteLinks, ServerOptions, TextLinoProtocol,
};
use link_cli::{Link, LinkStorage, NamedTypeLinks, QueryProcessor};
use std::sync::mpsc;
use std::time::Duration;
use tempfile::NamedTempFile;

const PUSH_TIMEOUT: Duration = Duration::from_secs(10);
const SILENCE_TIMEOUT: Duration = Duration::from_millis(200);

/// One step of `docs/protocol/subscriptions.txt`: a request and its reply, or
/// an expected push.
struct Step {
    connection: char,
    is_push: bool,
    message: &'static str,
    expected: String,
}

/// The steps of `docs/protocol/subscriptions.txt`, which the C# and
/// JavaScript tests replay too.
fn conversation() -> Vec<Step> {
    let starts = |line: &str| {
        let bytes = line.as_bytes();
        bytes.len() > 2
            && matches!(bytes[0], b'A' | b'B')
            && matches!(bytes[1], b'>' | b'<')
            && bytes[2] == b' '
    };
    let lines: Vec<&str> = include_str!("../../docs/protocol/subscriptions.txt")
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
fn servers_replay_the_shared_subscriptions_conversation_over_every_protocol() {
    let conversation = conversation();
    assert_eq!(conversation.len(), 24);
    for (first, second) in protocols().into_iter().zip(protocols()) {
        let server = RunningServer::start(ServerOptions::default(), true);
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
                assert_eq!(format_document(&push), step.expected, "{}", step.message);
                continue;
            }
            match step.expected.strip_prefix("! ") {
                Some(message) => {
                    let error = client.query_text(step.message).unwrap_err().to_string();
                    assert!(error.ends_with(message), "{}: {error}", step.message);
                }
                None => assert_eq!(
                    (step.message, client.query_text(step.message).unwrap()),
                    (step.message, step.expected.clone())
                ),
            }
            // Every push the server made before this reply arrived before it,
            // and the conversation lists none.
            assert_eq!(client.pending_notifications(), 0, "{}", step.message);
        }
        assert!(a.receive_notification(SILENCE_TIMEOUT).unwrap().is_none());
        assert!(b.receive_notification(SILENCE_TIMEOUT).unwrap().is_none());
    }
}

#[test]
fn a_subscriber_is_told_about_another_connections_writes_until_it_unsubscribes() {
    for (first, second) in protocols().into_iter().zip(protocols()) {
        let server = RunningServer::start(ServerOptions::default(), true);
        let subscriber = RemoteLinks::new(server.client(first));
        let writer = RemoteLinks::new(server.client(second));

        let subscription = subscriber.subscribe(&[None, None, None]).unwrap();
        assert_eq!(subscription, 1);
        writer
            .changes(&LinksOperation::Create {
                source: 0,
                target: 0,
            })
            .unwrap();

        assert_eq!(
            subscriber.receive_notification(PUSH_TIMEOUT).unwrap(),
            Some(LinksNotification {
                subscription: 1,
                changes: vec![(Link::null(), Link::new(1, 0, 0))],
            })
        );
        assert_eq!(
            subscriber.receive_notification(SILENCE_TIMEOUT).unwrap(),
            None
        );

        subscriber.unsubscribe(subscription).unwrap();
        writer
            .changes(&LinksOperation::Create {
                source: 0,
                target: 0,
            })
            .unwrap();
        assert_eq!(subscriber.count_matching(&[]).unwrap(), 2);
        assert_eq!(
            subscriber.receive_notification(SILENCE_TIMEOUT).unwrap(),
            None
        );
    }
}

#[test]
fn a_subscriber_sees_only_what_it_may_read() {
    // A link whose source is 2 is hidden: one that becomes hidden is gone, one
    // that stops being hidden is new.
    let server = RunningServer::start_with(ServerOptions::default(), false, |server| {
        server.with_readable(|link| link.source != 2)
    });
    let subscriber = RemoteLinks::new(server.client(TextLinoProtocol::new()));
    let mut writer = server.client(TextLinoProtocol::new());
    subscriber.subscribe(&[]).unwrap();

    for request in [
        "(create: (0 0))",
        "(update: (1 2 2))",
        "(update: (1 2 1))",
        "(update: (1 1 1))",
    ] {
        writer.query(request).unwrap();
    }

    let pushed = |before: Link, after: Link| {
        Some(LinksNotification {
            subscription: 1,
            changes: vec![(before, after)],
        })
    };
    let next = || subscriber.receive_notification(PUSH_TIMEOUT).unwrap();
    assert_eq!(next(), pushed(Link::null(), Link::new(1, 0, 0)));
    assert_eq!(next(), pushed(Link::new(1, 0, 0), Link::null()));
    // A change between two hidden states is not pushed at all.
    assert_eq!(next(), pushed(Link::null(), Link::new(1, 1, 1)));
    assert_eq!(
        subscriber.receive_notification(SILENCE_TIMEOUT).unwrap(),
        None
    );
}

#[test]
fn a_subscriber_is_pushed_only_what_its_subject_may_read() {
    // Issue #149: with permissions every connection is a session of its
    // subject, and its pushes are filtered by them.
    let (sent, received) = mpsc::channel();
    let server = RunningServer::start_serving(ServerOptions::default(), move |server, _| {
        let mut w = world::World::new(false).unwrap();
        let v = w.v.clone();
        sent.send((w.t.clone(), w.v.clone())).unwrap();
        let mut sessions = LinksSessions::with_permissions(&mut w.links, &mut w.policy)
            .with_authenticator(move |_, token| Ok(v.get(token).copied()));
        server.serve_sessions(&mut sessions).unwrap();
    });
    let (t, v) = received.recv().unwrap();
    let (alice, boss, note, secret) = (v["alice"], v["boss"], v["note"], v["secret"]);
    let mut alice_client = server.client(TextLinoProtocol::new());
    assert_eq!(alice, alice_client.authenticate("alice").unwrap());
    let subscriber = RemoteLinks::new(alice_client);
    let mut writer = server.client(TextLinoProtocol::new());
    assert_eq!(boss, writer.authenticate("boss").unwrap());
    subscriber.subscribe(&[]).unwrap();

    // alice may not read the secret: a change she cannot see is not pushed
    // to her at all...
    writer
        .query(&format!("(update: ({secret} {} {alice}))", t["Secret"]))
        .unwrap();
    // ...while one to a note she may read is.
    writer
        .query(&format!("(update: ({note} {} {alice}))", t["Note"]))
        .unwrap();

    assert_eq!(
        subscriber.receive_notification(PUSH_TIMEOUT).unwrap(),
        Some(LinksNotification {
            subscription: 1,
            changes: vec![(
                Link::new(note, t["Note"], note),
                Link::new(note, t["Note"], alice)
            )],
        })
    );
    assert_eq!(
        subscriber.receive_notification(SILENCE_TIMEOUT).unwrap(),
        None
    );
}

#[test]
fn a_subscriber_that_falls_too_far_behind_is_disconnected() {
    let server = RunningServer::start(
        ServerOptions {
            max_pending_notifications: 0,
            ..ServerOptions::default()
        },
        false,
    );
    let mut subscriber = server.client(TextLinoProtocol::new());
    let mut writer = server.client(TextLinoProtocol::new());
    subscriber.query("(subscribe: ())").unwrap();

    writer.query("(create: (0 0))").unwrap();

    let lost = subscriber
        .receive_notification(PUSH_TIMEOUT)
        .and_then(|_| subscriber.query("(count: ())"));
    assert!(lost.is_err(), "{lost:?}");
    assert_eq!(writer.query_text("(count: ())").unwrap(), "(count: 1)");
}

#[test]
fn subscriptions_need_a_server() {
    let database = NamedTempFile::new().unwrap();
    let mut storage = LinkStorage::new(database.path().to_str().unwrap(), false).unwrap();
    let processor = QueryProcessor::new(false);
    for request in ["(subscribe: ())", "(unsubscribe: 1)"] {
        let reply = execute_request(&mut storage, &processor, &parse_document(request).unwrap());
        assert_eq!(
            format_document(&reply),
            "(error: 'subscriptions belong to a connection to a links server')"
        );
    }
    assert!(LinksOperation::Subscribe(Vec::new())
        .execute(&mut storage)
        .is_err());
    assert_eq!(storage.all_links(), Vec::new());
    assert_eq!(
        format_document(&LinksOperation::Subscribe(vec![None, Some(1), None]).to_document()),
        "(subscribe: (* 1 *))"
    );
    assert_eq!(
        format_document(&LinksOperation::Unsubscribe(3).to_document()),
        "(unsubscribe: 3)"
    );
}

#[test]
fn only_notify_documents_are_pushes() {
    for (document, is_push) in [
        ("(notify: 1)", true),
        ("(notify: 2)\n() ((1: 0 0))", true),
        ("(notify: 1 2)", false),
        ("(notify: x)", false),
        ("(notify: (1 2))", false),
        ("(1: 2 2)", false),
        ("(count: 1)", false),
        ("", false),
    ] {
        assert_eq!(
            is_notification(&parse_document(document).unwrap()),
            is_push,
            "{document}"
        );
    }
}

#[test]
fn notifications_round_trip_through_their_documents() {
    let changes = vec![
        (Link::null(), Link::new(1, 0, 0)),
        (Link::new(2, 1, 1), Link::new(2, 1, 2)),
        (Link::new(3, 3, 3), Link::null()),
    ];
    let document = notification_document(7, &changes);
    let text = format_document(&document);

    assert_eq!(
        text,
        "(notify: 7)\n() ((1: 0 0))\n((2: 1 1)) ((2: 1 2))\n((3: 3 3)) ()"
    );
    assert_eq!(
        parse_notification(&parse_document(&text).unwrap()).unwrap(),
        LinksNotification {
            subscription: 7,
            changes,
        }
    );
    assert_eq!(parse_subscription(&subscription_document(3)).unwrap(), 3);
    let count = parse_document("(count: 1)").unwrap();
    assert!(matches!(
        parse_notification(&count),
        Err(ProtocolError::Malformed(_))
    ));
    assert!(matches!(
        parse_subscription(&count),
        Err(ProtocolError::Malformed(_))
    ));
}
