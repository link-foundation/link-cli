//! [`RemoteLinks`] is a drop-in replacement for a local store: every protocol
//! gives the same results as the store it talks to.

mod common;

use common::{protocols, RunningServer};
use doublets::data::Flow;
use doublets::{Doublets, Error, Link as DoubletsLink, Links};
use link_cli::protocol::links_operations::{
    matches, parse_changes, parse_count, parse_link_reply, parse_links, parse_name,
};
use link_cli::protocol::{
    parse_document, LinksOperation, LinoProtocol, ProtocolError, RemoteLinks, ServerOptions,
    TextLinoProtocol,
};
use link_cli::{simplify_changes, Link, NamedTypeLinks, NamedTypesDecorator, QueryProcessor};
use std::net::TcpListener;
use std::panic::{self, AssertUnwindSafe};
use std::thread;
use tempfile::NamedTempFile;

/// Changes as the query processor reports them.
type Changes = Vec<(Option<Link>, Option<Link>)>;

/// The README walkthrough plus the cases that cascade: a merge into an
/// existing doublet and the deletion of a link that others use.
const QUERIES: &[&str] = &[
    "() ((1 1))",
    "() ((2 2))",
    "((1: 1 1)) ((1: 1 2))",
    "((1: 1 2)) ((1: 2 1))",
    "((($i: $s $t)) (($i: $t $s)))",
    "((2: 2 2)) ((2: 1 1))",
    "() ((father: father father))",
    "() ((mother: mother mother))",
    "() ((child: father mother))",
    "((child: father mother)) ((child: father mother))",
    "((child: father mother)) ((child: mother father))",
    "((father: father father)) ()",
    "((1 *)) ()",
];

fn local_store() -> (NamedTypesDecorator, NamedTempFile, NamedTempFile) {
    let database = NamedTempFile::new().unwrap();
    let names = NamedTempFile::new().unwrap();
    let store = NamedTypesDecorator::with_names_database_path(database.path(), names.path(), false)
        .unwrap();
    (store, database, names)
}

fn connect(server: &RunningServer, protocol: impl LinoProtocol + 'static) -> RemoteLinks {
    RemoteLinks::new(server.client(protocol))
}

fn run_queries(store: &mut impl NamedTypeLinks) -> Vec<Changes> {
    let processor = QueryProcessor::new(false).with_auto_create_missing_references(true);
    QUERIES
        .iter()
        .map(|query| processor.process_query(store, query).unwrap())
        .collect()
}

#[test]
fn the_query_processor_gives_the_same_results_on_a_remote_store() {
    let (mut local, _database, _names) = local_store();
    let expected_changes = run_queries(&mut local);
    let expected_lines = local.lino_lines().unwrap();

    for protocol in protocols() {
        let server = RunningServer::start(ServerOptions::default(), true);
        let mut remote = connect(&server, protocol);

        assert_eq!(run_queries(&mut remote), expected_changes);
        assert_eq!(remote.lino_lines().unwrap(), expected_lines);
    }
}

#[test]
fn every_named_types_links_call_matches_a_local_store() {
    for protocol in protocols() {
        let (mut local, _database, _names) = local_store();
        let server = RunningServer::start(ServerOptions::default(), true);
        let mut remote = connect(&server, protocol);

        for store in [&mut local as &mut dyn NamedTypeLinksCalls, &mut remote] {
            store.exercise();
        }
        assert_eq!(remote.all_links(), local.all_links());
        assert_eq!(remote.lino_lines().unwrap(), local.lino_lines().unwrap());
    }
}

/// Runs every [`NamedTypeLinks`] call once and checks what a local store
/// returns, so the remote store is held to exactly the same answers.
trait NamedTypeLinksCalls {
    fn exercise(&mut self);
}

impl<S: NamedTypeLinks> NamedTypeLinksCalls for S {
    fn exercise(&mut self) {
        assert_eq!(self.create(0, 0), 1);
        assert_eq!(self.get_or_create(1, 1), 2);
        assert_eq!(self.get_or_create(1, 1), 2);
        assert_eq!(self.search(1, 1), Some(2));
        assert_eq!(self.search(2, 2), None);
        assert_eq!(self.get_link(2), Some(Link::new(2, 1, 1)));
        assert_eq!(self.get_link(9), None);
        assert!(self.exists(1));
        assert!(!self.exists(9));

        assert_eq!(self.ensure_created(5), 5);
        assert_eq!(self.ensure_created(5), 5);
        assert!(!self.exists(3) && !self.exists(4));
        assert_eq!(self.try_ensure_created(0).ok(), None);

        assert_eq!(self.update(5, 2, 2).unwrap(), Link::new(5, 0, 0));
        assert!(self.update(9, 1, 1).is_err());

        let mut changes = Vec::new();
        let merged = self
            .update_observed(5, 1, 1, &mut |before, after| changes.push((before, after)))
            .unwrap();
        assert_eq!(merged, Link::new(5, 2, 2));
        // A server replies with the net change of every link it touched.
        assert_eq!(
            simplify_changes(changes),
            vec![(Link::new(5, 2, 2), Link::null())]
        );
        assert!(!self.exists(5));

        self.set_name(2, "pair").unwrap();
        assert_eq!(self.get_name(2).unwrap().as_deref(), Some("pair"));
        assert_eq!(self.get_by_name("pair").unwrap(), Some(2));
        assert_eq!(self.get_by_name("none").unwrap(), None);
        assert_eq!(
            self.format_lino(&Link::new(2, 1, 1)).unwrap(),
            "(pair: 1 1)"
        );
        self.remove_name(2).unwrap();
        assert_eq!(self.get_name(2).unwrap(), None);

        let named = self.get_or_create_named("leaf").unwrap();
        assert_eq!(self.get_link(named), Some(Link::new(named, named, named)));

        let mut deleted = Vec::new();
        let removed = self
            .delete_observed(1, &mut |before, after| deleted.push((before, after)))
            .unwrap();
        assert_eq!(removed, Link::new(1, 0, 0));
        assert!(deleted.contains(&(Link::new(1, 0, 0), Link::null())));
        assert!(deleted.contains(&(Link::new(2, 1, 1), Link::null())));
        assert!(self.delete(1).is_err());
        self.save().unwrap();
    }
}

#[test]
fn the_raw_links_interface_works_over_every_protocol() {
    for protocol in protocols() {
        let server = RunningServer::start(ServerOptions::default(), false);
        let mut remote = connect(&server, protocol);
        let any = remote.constants().any;

        let mut created = Vec::new();
        let flow = Links::create_links(&mut remote, &[], &mut |before, after| {
            created.push((before, after));
            Flow::Continue
        })
        .unwrap();
        assert_eq!(flow, Flow::Continue);
        assert_eq!(
            created,
            vec![(DoubletsLink::new(0, 0, 0), DoubletsLink::new(1, 0, 0))]
        );
        Links::create_links(&mut remote, &[], &mut |_, _| Flow::Continue).unwrap();

        Links::update_links(&mut remote, &[1], &[1, 1, 2], &mut |_, _| Flow::Continue).unwrap();
        Links::update_links(&mut remote, &[2], &[2, 2, 1], &mut |_, _| Flow::Continue).unwrap();
        assert_eq!(
            Doublets::get_link(&remote, 1),
            Some(DoubletsLink::new(1, 1, 2))
        );
        assert_eq!(Doublets::get_link(&remote, 3), None);

        assert_eq!(remote.count_links(&[]), 2);
        assert_eq!(remote.count_links(&[any]), 2);
        assert_eq!(remote.count_links(&[2]), 1);
        assert_eq!(remote.count_links(&[any, 1]), 2);
        assert_eq!(remote.count_links(&[any, 1, any]), 1);
        assert_eq!(remote.count_links(&[any, any, 1]), 1);
        assert_eq!(remote.count_links(&[any, 2, 2]), 0);

        let mut seen = Vec::new();
        let flow = remote.each_links(&[any, any, any], &mut |link| {
            seen.push(link);
            Flow::Break
        });
        assert_eq!(
            (flow, seen),
            (Flow::Break, vec![DoubletsLink::new(1, 1, 2)])
        );

        let mut deleted = Vec::new();
        let flow = Links::delete_links(&mut remote, &[1], &mut |before, after| {
            deleted.push((before, after));
            Flow::Break
        })
        .unwrap();
        assert_eq!(flow, Flow::Break);
        assert_eq!(deleted.len(), 1);
        assert_eq!(remote.count_links(&[]), 0);
    }
}

#[test]
fn server_failures_are_errors_not_panics() {
    let server = RunningServer::start(ServerOptions::default(), false);
    let mut remote = connect(&server, TextLinoProtocol::new());

    let error =
        Links::update_links(&mut remote, &[7], &[7, 1, 1], &mut |_, _| Flow::Continue).unwrap_err();
    assert!(matches!(error, Error::Other(_)), "{error:?}");
    assert!(Links::delete_links(&mut remote, &[7], &mut |_, _| Flow::Continue).is_err());
    assert!(NamedTypeLinks::update(&mut remote, 7, 1, 1).is_err());
    assert!(remote
        .count_matching(&[Some(1), Some(2), Some(3), Some(4)])
        .is_err());
}

#[test]
fn a_lost_connection_panics_where_the_interface_cannot_fail() {
    let mut server = RunningServer::start(ServerOptions::default(), false);
    let remote = connect(&server, TextLinoProtocol::new());
    server.stop();

    assert!(remote.count_matching(&[]).is_err());
    let panic = panic::catch_unwind(AssertUnwindSafe(|| remote.count_links(&[])))
        .expect_err("a lost connection cannot be counted");
    let message = panic.downcast_ref::<String>().unwrap();
    assert!(
        message.starts_with("remote links store failed"),
        "{message}"
    );
}

#[test]
fn operations_round_trip_through_their_documents() {
    let operations = [
        LinksOperation::Count(vec![]),
        LinksOperation::Count(vec![Some(1)]),
        LinksOperation::Each(vec![None, Some(2)]),
        LinksOperation::Each(vec![Some(1), None, Some(3)]),
        LinksOperation::Create {
            source: 1,
            target: 2,
        },
        LinksOperation::Update {
            index: 3,
            source: 4,
            target: 5,
        },
        LinksOperation::Delete(6),
        LinksOperation::GetName(7),
        LinksOperation::SetName(8, "a name".to_string()),
        LinksOperation::GetByName("a name".to_string()),
        LinksOperation::RemoveName(9),
    ];
    for protocol in protocols() {
        for operation in &operations {
            let mut bytes = Vec::new();
            protocol
                .write_document(&mut bytes, &operation.to_document())
                .unwrap();
            let document = protocol
                .read_document(&mut bytes.as_slice())
                .unwrap()
                .unwrap();
            assert_eq!(
                LinksOperation::from_document(&document).unwrap().as_ref(),
                Some(operation)
            );
        }
    }
}

#[test]
fn only_operation_shaped_documents_are_operations() {
    let parse = |text: &str| LinksOperation::from_document(&parse_document(text).unwrap());

    for query in [
        "() ((1 1))",
        "((1: 1 1)) ()",
        "(unknown: 1)",
        "(count: 1 2)",
    ] {
        assert_eq!(parse(query).unwrap(), None, "{query}");
    }
    for malformed in [
        "(count: (1 2 3 4))",
        "(each: (x))",
        "(create: (1))",
        "(update: (1 2))",
        "(delete: x)",
        "(delete: -1)",
        "(get-name: (1 2))",
        "(set-name: (1))",
        "(set-name: (1 (a b)))",
        "(get-by-name: (a b))",
        "(remove-name: 4294967296)",
    ] {
        assert!(
            matches!(parse(malformed), Err(ProtocolError::Malformed(_))),
            "{malformed}"
        );
    }
}

#[test]
fn replies_parse_back_to_what_they_carry() {
    let parse = |text: &str| parse_document(text).unwrap();

    assert_eq!(parse_count(&parse("(count: 3)")).unwrap(), 3);
    assert_eq!(
        parse_links(&parse("(1: 2 3)")).unwrap(),
        vec![Link::new(1, 2, 3)]
    );
    assert_eq!(
        parse_changes(&parse("() ((1: 0 0))\n((1: 0 0)) ()")).unwrap(),
        vec![
            (Link::null(), Link::new(1, 0, 0)),
            (Link::new(1, 0, 0), Link::null())
        ]
    );
    // A side that is a single named link may lose its wrapper.
    assert_eq!(
        parse_changes(&parse("(1: 1 1) (1: 1 2)")).unwrap(),
        vec![(Link::new(1, 1, 1), Link::new(1, 1, 2))]
    );
    assert_eq!(
        parse_name(&parse("(name: 'a name')")).unwrap().as_deref(),
        Some("a name")
    );
    assert_eq!(parse_name(&[]).unwrap(), None);
    assert_eq!(parse_link_reply(&parse("(link: 5)")).unwrap(), Some(5));
    assert_eq!(parse_link_reply(&[]).unwrap(), None);
}

#[test]
fn malformed_replies_are_rejected() {
    for reply in [
        "(count: 1 2)",
        "(count: x)",
        "(name: a b)",
        "(link: 1 2)",
        "(1 2)",
        "((1 2) x)",
    ] {
        let document = parse_document(reply).unwrap();
        for result in [
            parse_count(&document).map(|_| ()),
            parse_links(&document).map(|_| ()),
            parse_changes(&document).map(|_| ()),
            parse_name(&document).map(|_| ()),
            parse_link_reply(&document).map(|_| ()),
        ] {
            assert!(
                matches!(result, Err(ProtocolError::Malformed(_))),
                "{reply}: {result:?}"
            );
        }
    }
    // Each kind of misshapen change side and link.
    for (reply, detail) in [
        ("(1 2) ()", "expected a change side, found (1 2)"),
        ("((1 2)) ()", "expected (index: source target), found (1 2)"),
        (
            "((1: 2)) ()",
            "expected (index: source target), found (1: 2)",
        ),
        ("((x: 1 2)) ()", "expected a number, found 'x'"),
        ("((1: (2 3) 4)) ()", "expected a number, found (2 3)"),
    ] {
        match parse_changes(&parse_document(reply).unwrap()) {
            Err(ProtocolError::Malformed(message)) => {
                assert!(message.contains(detail), "{reply}: {message}")
            }
            other => panic!("{reply}: {other:?}"),
        }
    }
}

/// A server that answers every request with `reply`, whatever it asks.
fn scripted_server(reply: &'static str) -> RemoteLinks {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
        let mut writer = stream;
        let protocol = TextLinoProtocol::new();
        let reply = parse_document(reply).unwrap();
        while let Ok(Some(_)) = protocol.read_document(&mut reader) {
            if protocol.write_document(&mut writer, &reply).is_err() {
                return;
            }
        }
    });
    RemoteLinks::connect(address, TextLinoProtocol::new()).unwrap()
}

#[test]
fn replies_that_miss_the_requested_change_are_errors() {
    // An empty reply is a valid document that reports no change at all.
    let mut remote = scripted_server("");
    let error = |result: anyhow::Result<Link>| result.unwrap_err().to_string();
    assert_eq!(
        error(remote.update_observed(1, 1, 1, &mut |_, _| {})),
        "malformed message: the reply holds no change of 1"
    );
    assert_eq!(
        error(remote.delete_observed(2, &mut |_, _| {})),
        "malformed message: the reply holds no change of 2"
    );
    assert_eq!(
        NamedTypeLinks::set_name(&mut remote, 1, "x")
            .unwrap_err()
            .to_string(),
        "the set-name reply holds no link"
    );
    assert_eq!(NamedTypeLinks::get_by_name(&mut remote, "x").unwrap(), None);
    let panic = panic::catch_unwind(AssertUnwindSafe(|| {
        NamedTypeLinks::create(&mut remote, 1, 1)
    }))
    .expect_err("a create must create a link");
    let message = panic.downcast_ref::<String>().unwrap();
    assert!(
        message.ends_with("the create reply holds no created link"),
        "{message}"
    );
}

#[test]
fn reads_stop_where_the_handler_breaks() {
    let server = RunningServer::start(ServerOptions::default(), false);
    let mut remote = RemoteLinks::connect(server.address, TextLinoProtocol::new()).unwrap();
    NamedTypeLinks::create(&mut remote, 1, 1);
    NamedTypeLinks::create(&mut remote, 2, 2);
    let mut visited = 0;
    let flow = remote.each_links(&[], &mut |_| {
        visited += 1;
        Flow::Break
    });
    assert_eq!((flow, visited), (Flow::Break, 1));
    let mut visited = 0;
    let flow = remote.each_links(&[], &mut |_| {
        visited += 1;
        Flow::Continue
    });
    assert_eq!((flow, visited), (Flow::Continue, 2));
    assert_eq!(remote.count_links(&[]), 2);
}

#[test]
fn misshapen_name_replies_are_errors() {
    let mut remote = scripted_server("(count: 1)");
    assert_eq!(
        NamedTypeLinks::get_name(&mut remote, 1)
            .unwrap_err()
            .to_string(),
        "malformed message: expected a name reply, found (count: 1)"
    );
    assert_eq!(
        NamedTypeLinks::get_by_name(&mut remote, "x")
            .unwrap_err()
            .to_string(),
        "malformed message: expected a link reply, found (count: 1)"
    );
}

#[test]
fn restrictions_match_like_the_raw_links_interface() {
    let link = Link::new(1, 2, 3);
    let shapes: &[(&[Option<u32>], bool)] = &[
        (&[], true),
        (&[None], true),
        (&[Some(1)], true),
        (&[Some(2)], false),
        (&[None, Some(2)], true),
        (&[None, Some(3)], true),
        (&[None, Some(1)], false),
        (&[Some(1), Some(2), Some(3)], true),
        (&[None, Some(3), None], false),
        (&[None, None, None, None], false),
    ];
    for (restriction, expected) in shapes {
        assert_eq!(matches(&link, restriction), *expected, "{restriction:?}");
    }
}

/// The requests and replies of `docs/protocol/links-operations.txt`, which
/// the C# tests replay against the C# server too.
fn conversation() -> Vec<(&'static str, String)> {
    let lines: Vec<&str> = include_str!("../../docs/protocol/links-operations.txt")
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    let mut exchanges = Vec::new();
    let mut start = 0;
    while start < lines.len() {
        let end = (start + 1..lines.len())
            .find(|&line| lines[line].starts_with("> "))
            .unwrap_or(lines.len());
        exchanges.push((&lines[start][2..], lines[start + 1..end].join("\n")));
        start = end;
    }
    exchanges
}

#[test]
fn servers_answer_the_shared_conversation_over_every_protocol() {
    let conversation = conversation();
    assert_eq!(conversation.len(), 19);
    for protocol in protocols() {
        let server = RunningServer::start(ServerOptions::default(), true);
        let mut client = server.client(protocol);
        for (request, reply) in &conversation {
            match reply.strip_prefix("! ") {
                Some(message) => {
                    let error = client.query_text(request).unwrap_err().to_string();
                    assert!(error.ends_with(message), "{request}: {error}");
                }
                None => assert_eq!(
                    (*request, client.query_text(request).unwrap()),
                    (*request, reply.clone())
                ),
            }
        }
    }
}
