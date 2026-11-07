//! Diagnostic errors must not interrupt the existing storage decorators.
mod common;

#[cfg(target_os = "linux")]
#[test]
fn failed_stderr_preserves_raw_transaction_and_version_operations() {
    let directory = tempfile::tempdir().unwrap();
    let mut failures = Vec::new();
    for mode in [
        "raw", "named", "query", "record", "commit", "rollback", "sized", "chunked", "version",
        "server",
    ] {
        let path = directory.path().join(mode);
        std::fs::create_dir(&path).unwrap();
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "diagnostic_failure_child",
                "--ignored",
                "--nocapture",
            ])
            .env("LINK_DIAGNOSTIC_PATH", &path)
            .env("LINK_DIAGNOSTIC_MODE", mode)
            .stderr(
                std::fs::OpenOptions::new()
                    .write(true)
                    .open("/dev/full")
                    .unwrap(),
            )
            .output()
            .unwrap();
        if !child.status.success() {
            failures.push(format!(
                "{mode}: {}\n{}",
                child.status,
                String::from_utf8_lossy(&child.stdout)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
#[ignore = "child process entry point"]
fn diagnostic_failure_child() {
    use link_cli::{
        CommitMode, FileTransitionLog, GenericTransactionsDecorator, LinkStorage,
        LogRetentionPolicy, NamedTypes, NamedTypesDecorator, QueryProcessor, TransactionsDecorator,
        VersionControlDecorator,
    };
    let Some(path) = std::env::var_os("LINK_DIAGNOSTIC_PATH") else {
        return;
    };
    let path = std::path::PathBuf::from(path);
    let mode = std::env::var("LINK_DIAGNOSTIC_MODE").unwrap();
    match mode.as_str() {
        "server" => {
            use link_cli::protocol::{
                format_document, parse_document, LinoConnection, ServerOptions, TextLinoProtocol,
            };
            let mut server = common::RunningServer::start(
                ServerOptions {
                    trace: true,
                    ..ServerOptions::default()
                },
                false,
            );
            let stream = std::net::TcpStream::connect(server.address).unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            stream
                .set_write_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut client = LinoConnection::new(stream, TextLinoProtocol::new());
            let replies: Result<Vec<_>, _> = ["() ((1 1))", ""]
                .into_iter()
                .map(|query| {
                    client
                        .request(&parse_document(query).unwrap())
                        .map(|reply| format_document(&reply))
                })
                .collect();
            drop(client);
            // Join before checking replies so a failed server cannot trigger
            // a second panic in its Drop implementation during assertion unwind.
            server.stop();
            assert_eq!(replies.unwrap(), ["() ((1: 1 1))", "(1: 1 1)"]);
        }
        "raw" => {
            let data = path.join("data");
            let mut store = LinkStorage::new(&data, true)
                .unwrap()
                .with_indexed_lookups();
            let id = store.create(1, 2);
            store.ensure_created(8);
            store.update(id, 2, 1).unwrap();
            store.set_name(id, "profile");
            assert_eq!(store.get_by_name("profile"), Some(id));
            store.remove_name(id);
            let named = store.get_or_create_named("saved");
            store.delete(id).unwrap();
            store.save().unwrap();
            let reopened = LinkStorage::new(data, true).unwrap();
            assert_eq!(reopened.get_by_name("saved"), Some(named));
            assert!(reopened.get(id).is_none());
        }
        "named" => {
            let data = path.join("data");
            let mut store = NamedTypesDecorator::new(&data, true).unwrap();
            assert_eq!(store.get_by_name("absent").unwrap(), None);
            let id = store.create(1, 2);
            store.set_name(id, "profile").unwrap();
            assert_eq!(store.get_name(id).unwrap().as_deref(), Some("profile"));
            assert_eq!(store.get_by_name("profile").unwrap(), Some(id));
            store.remove_name(id).unwrap();
            assert_eq!(store.get_name(id).unwrap(), None);
            store.save().unwrap();
        }
        "query" => {
            let mut store = NamedTypesDecorator::new(path.join("data"), false).unwrap();
            QueryProcessor::new(true)
                .with_auto_create_missing_references(true)
                .process_query(&mut store, "(() ((1: 1 1)))")
                .unwrap();
            assert_eq!(store.get(1).unwrap().target, 1);
        }
        "version" => {
            let data = NamedTypesDecorator::new(path.join("data"), false).unwrap();
            let log = NamedTypesDecorator::new(path.join("log"), false).unwrap();
            let tx = TransactionsDecorator::new(
                data,
                log,
                LogRetentionPolicy::Infinite,
                CommitMode::Sync,
                false,
            )
            .unwrap();
            let branches = NamedTypesDecorator::new(path.join("branches"), false).unwrap();
            let mut vc = VersionControlDecorator::new(tx, branches, true).unwrap();
            let id = vc.create_and_update(0, 0).unwrap();
            vc.branch("feature", None).unwrap();
            vc.switch_branch("feature").unwrap();
            vc.tag("saved", None).unwrap();
            let head = vc.current_sequence();
            vc.checkout(0).unwrap();
            assert!(!vc.exists(id));
            vc.checkout(head).unwrap();
            assert!(vc.exists(id));
            assert_eq!(vc.try_get_tag("saved"), Some(head));
            vc.save().unwrap();
        }
        _ => {
            let data = path.join("data");
            let log = path.join("log");
            let policy = match mode.as_str() {
                "sized" => LogRetentionPolicy::Sized { max_transitions: 1 },
                "chunked" => LogRetentionPolicy::Chunked {
                    chunk_size: 1,
                    archive_directory: path.join("archive"),
                },
                _ => LogRetentionPolicy::Infinite,
            };
            let mut tx = GenericTransactionsDecorator::new(
                LinkStorage::new(&data, false).unwrap(),
                FileTransitionLog::open(&log).unwrap(),
                policy,
                CommitMode::Sync,
                true,
            )
            .unwrap();
            // Empty transaction cases reach their own trace before any record trace.
            if mode == "commit" {
                tx.begin_transaction().unwrap();
                tx.commit().unwrap();
            }
            if mode == "rollback" {
                tx.begin_transaction().unwrap();
                tx.rollback().unwrap();
            }
            tx.begin_transaction().unwrap();
            let retained = tx.create(0, 0).unwrap();
            tx.commit().unwrap();
            tx.begin_transaction().unwrap();
            tx.update(retained, retained, 0).unwrap();
            tx.commit().unwrap();
            tx.begin_transaction().unwrap();
            let removed = tx.create(0, 0).unwrap();
            tx.rollback().unwrap();
            assert!(tx.exists(retained));
            assert!(!tx.exists(removed));
            if mode == "sized" {
                assert_eq!(tx.log().len(), 1);
            }
            if mode == "chunked" {
                assert!(tx.log().is_empty());
                assert!(std::fs::read_dir(path.join("archive")).unwrap().count() >= 2);
            }
            drop(tx);
            let reopened = GenericTransactionsDecorator::new(
                LinkStorage::new(data, false).unwrap(),
                FileTransitionLog::open(log).unwrap(),
                LogRetentionPolicy::Infinite,
                CommitMode::Sync,
                false,
            )
            .unwrap();
            assert!(reopened.exists(retained));
            assert!(!reopened.exists(removed));
        }
    }
}
