use super::*;
use crate::link_storage::LinkStorage;
use links_notation::binary::parse_document;

fn outbox() -> Arc<Outbox> {
    Arc::new(Outbox::new(
        DEFAULT_MAX_PENDING_NOTIFICATIONS,
        Box::new(|| {}),
    ))
}

fn request(
    dispatcher: &mut Dispatcher,
    sessions: &mut LinksSessions<LinkStorage>,
    connection: u64,
    outbox: &Arc<Outbox>,
    text: &str,
) {
    let (done, finished) = mpsc::channel();
    let job = Job::Request {
        connection,
        outbox: Arc::clone(outbox),
        document: parse_document(text).unwrap(),
        format: MessageFormat::Text,
        done,
    };
    assert!(dispatcher.run(sessions, job));
    finished.recv().unwrap();
}

fn queued(outbox: &Outbox) -> Vec<String> {
    outbox
        .lock()
        .queue
        .drain(..)
        .map(|(document, _, _)| format_document(&document))
        .collect()
}

#[test]
fn closing_a_connection_drops_its_subscriptions() {
    let database = tempfile::NamedTempFile::new().unwrap();
    let storage = LinkStorage::new(database.path(), false).unwrap();
    let mut sessions = LinksSessions::new(storage);
    let mut dispatcher = Dispatcher::new(ServerOptions::default(), None);
    let (subscriber, writer) = (outbox(), outbox());

    request(
        &mut dispatcher,
        &mut sessions,
        1,
        &subscriber,
        "(subscribe: ())",
    );
    request(
        &mut dispatcher,
        &mut sessions,
        2,
        &writer,
        "(create: (0 0))",
    );
    assert_eq!(dispatcher.connections.len(), 2);
    assert_eq!(sessions.sessions().len(), 2);
    assert_eq!(
        queued(&subscriber),
        ["(subscription: 1)", "(notify: 1)\n() ((1: 0 0))"]
    );
    // A connection is never told about its own writes.
    assert_eq!(queued(&writer), ["() ((1: 0 0))"]);

    assert!(dispatcher.run(&mut sessions, Job::Close(1)));
    assert_eq!(dispatcher.connections.keys().collect::<Vec<_>>(), [&2]);
    // Its session is closed with it.
    assert_eq!(sessions.sessions().len(), 1);
    request(
        &mut dispatcher,
        &mut sessions,
        2,
        &writer,
        "(create: (1 1))",
    );
    assert!(queued(&subscriber).is_empty());
    assert!(!dispatcher.run(&mut sessions, Job::Shutdown));
    dispatcher.close_all(&mut sessions);
    assert!(sessions.sessions().is_empty());
}
