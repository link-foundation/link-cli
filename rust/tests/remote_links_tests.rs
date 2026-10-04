//! [`RemoteLinks`] is a drop-in replacement for a local store: every protocol
//! gives the same results as the store it talks to.

mod common;

use common::{protocols, RunningServer};
use doublets::data::Flow;
use doublets::{Doublets, Error, Link as DoubletsLink, Links};
use link_cli::protocol::{
    links_operations::matches, parse_document, LinksOperation, LinoProtocol, RemoteLinks,
    ServerOptions, TextLinoProtocol,
};
use link_cli::{Link, NamedTypeLinks, NamedTypesDecorator, QueryProcessor};
use std::panic::{self, AssertUnwindSafe};
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
        assert_eq!(
            changes,
            vec![
                (Link::new(5, 2, 2), Link::new(5, 0, 0)),
                (Link::new(5, 0, 0), Link::null())
            ]
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
        "(set-name: (1))",
        "(get-by-name: (a b))",
    ] {
        assert!(parse(malformed).is_err(), "{malformed}");
    }
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
