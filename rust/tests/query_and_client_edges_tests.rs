//! Edges of the query processor and of the protocol client: query shapes that
//! ask for nothing, restated links, and replies a client has to refuse.

use std::net::TcpListener;
use std::thread;
use std::time::Duration;

use anyhow::Result;
use link_cli::protocol::{
    parse_document, LinksClient, LinoProtocol, ProtocolError, TextLinoProtocol,
};
use link_cli::{Link, LinkStorage, QueryOptions, QueryProcessor};

fn processor() -> QueryProcessor {
    QueryProcessor::new(true).with_auto_create_missing_references(true)
}

fn rows(storage: &LinkStorage) -> Vec<Link> {
    let mut rows: Vec<Link> = storage.all().into_iter().copied().collect();
    rows.sort_by_key(|link| link.index);
    rows
}

#[test]
fn a_query_with_nothing_to_do_changes_nothing() -> Result<()> {
    let mut storage = LinkStorage::in_memory(false);
    assert!(processor().process_query(&mut storage, "")?.is_empty());
    // A single link with fewer than two values holds no substitution.
    assert!(processor().process_query(&mut storage, "(1)")?.is_empty());
    assert!(processor().process_query(&mut storage, "(() ())")?.is_empty());
    assert!(rows(&storage).is_empty());
    Ok(())
}

#[test]
fn restriction_and_substitution_may_be_separate_links() -> Result<()> {
    let mut storage = LinkStorage::in_memory(false);
    let changes = processor().process_query(&mut storage, "()\n((1: 1 1))")?;
    assert_eq!(changes.len(), 1);
    assert_eq!(rows(&storage), [Link::new(1, 1, 1)]);
    Ok(())
}

#[test]
fn restating_an_addressed_link_reports_it_unchanged() -> Result<()> {
    let mut storage = LinkStorage::in_memory(false);
    processor().process_query(&mut storage, "(() ((1: 1 1)))")?;
    let changes = processor().process_query(&mut storage, "(() ((1: 1 1)))")?;
    assert!(changes
        .iter()
        .all(|(before, after)| before == after && before.is_some()));
    assert_eq!(rows(&storage), [Link::new(1, 1, 1)]);
    let same = processor().process_query(&mut storage, "(((1: 1 1)) ((1: 1 1)))")?;
    assert!(same.iter().all(|(before, after)| before == after));
    assert_eq!(rows(&storage), [Link::new(1, 1, 1)]);
    Ok(())
}

#[test]
fn a_restated_doublet_without_an_address_is_found_not_copied() -> Result<()> {
    let mut storage = LinkStorage::in_memory(false);
    processor().process_query(&mut storage, "(() ((1: 1 1)))")?;
    processor().process_query(&mut storage, "(() ((1 1)))")?;
    assert_eq!(rows(&storage), [Link::new(1, 1, 1)]);
    Ok(())
}

#[test]
fn a_named_or_variable_address_makes_a_doublet() -> Result<()> {
    let mut storage = LinkStorage::in_memory(false);
    processor().process_query(&mut storage, "(() ((1: 1 1)))")?;
    processor().process_query(&mut storage, "(() ((*: 1 1)))")?;
    assert_eq!(rows(&storage), [Link::new(1, 1, 1)]);
    processor().process_query(&mut storage, "(() ((pair: 1 1)))")?;
    assert_eq!(storage.get_by_name("pair"), Some(1));
    processor().process_query(&mut storage, "(() ((pair: 1 1)))")?;
    assert_eq!(rows(&storage), [Link::new(1, 1, 1)]);
    Ok(())
}

#[test]
fn a_nested_link_of_the_wrong_shape_is_refused() {
    let mut storage = LinkStorage::in_memory(false);
    let refused = processor().process_query(&mut storage, "(() (((1 2 3) 1)))");
    assert!(refused.is_err(), "{refused:?}");
}

#[test]
fn query_options_keep_what_they_are_given() {
    let options = QueryOptions::new("(() ())", true);
    assert_eq!(options.query, "(() ())");
    assert!(options.trace);
}

/// A server that answers the n-th request with the n-th reply text, sending
/// each reply's documents in order.
fn scripted(replies: &'static [&'static [&'static str]]) -> LinksClient {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
        let mut writer = stream;
        let protocol = TextLinoProtocol::new();
        for documents in replies {
            for text in *documents {
                let reply = parse_document(text).unwrap();
                protocol.write_document(&mut writer, &reply).unwrap();
            }
            if !matches!(protocol.read_document(&mut reader), Ok(Some(_))) {
                return;
            }
        }
        // Hold the connection until the client has read everything.
        let _ = protocol.read_document(&mut reader);
    });
    LinksClient::connect(address, TextLinoProtocol::new()).unwrap()
}

#[test]
fn a_notification_before_a_reply_is_kept_for_later() {
    // The first request sends a push followed by the reply it is waiting for.
    let mut client = scripted(&[&["(notify: 1)", "(reply)"], &[]]);
    let document = parse_document("(any)").unwrap();
    assert_eq!(client.request(&document).unwrap().len(), 1);
    assert_eq!(client.pending_notifications(), 1);
    let queued = client
        .receive_notification(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    assert_eq!(queued, parse_document("(notify: 1)").unwrap());
    assert_eq!(client.pending_notifications(), 0);
}

#[test]
fn a_reply_nobody_asked_for_is_malformed() {
    let mut client = scripted(&[&["(reply)"], &[]]);
    let error = client
        .receive_notification(Duration::from_secs(5))
        .unwrap_err();
    assert!(
        matches!(&error, ProtocolError::Malformed(text) if text.contains("nobody asked for")),
        "{error:?}"
    );
}

#[test]
fn a_subject_reply_of_any_other_shape_is_malformed() {
    for reply in [
        "(subject: x)",
        "(other: 5)",
        "(subject: 99999999999999999999)",
        "(subject: 1 2)",
        "subject",
    ] {
        let replies: &'static [&'static str] = Box::leak(Box::new([reply]));
        let scripts: &'static [&'static [&'static str]] = Box::leak(Box::new([replies, &[]]));
        let mut client = scripted(scripts);
        let error = client.authenticate("token").unwrap_err();
        assert!(
            matches!(&error, ProtocolError::Malformed(text) if text.contains("subject")),
            "{reply}: {error:?}"
        );
    }
}
