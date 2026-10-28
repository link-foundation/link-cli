//! Public command contracts for optional transaction and version policies.

mod common;

use common::RunningServer;
use link_cli::cli::Cli;
use link_cli::protocol::ServerOptions;
use std::fs::File;
use std::io::{Read, Seek};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::{tempdir, tempfile};

struct Reap(Child);

impl Drop for Reap {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Reply {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

impl Reply {
    fn success(self) -> String {
        assert!(self.status.success(), "{}", self.stderr);
        self.stdout
    }

    fn failure(self, message: &str) {
        assert_eq!(self.status.code(), Some(1), "{}", self.stdout);
        assert!(self.stderr.contains(message), "{}", self.stderr);
    }
}

fn read(mut file: File) -> String {
    file.rewind().unwrap();
    let mut text = String::new();
    file.take(256 * 1024).read_to_string(&mut text).unwrap();
    text
}

fn run(db: &Path, args: &[&str]) -> Reply {
    // Files prevent a full child pipe from blocking deadline/reaping logic.
    let stdout = tempfile().unwrap();
    let stderr = tempfile().unwrap();
    let mut child = Reap(
        Command::new(env!("CARGO_BIN_EXE_clink"))
            .arg("--db")
            .arg(db)
            .args(args)
            .stdin(Stdio::null())
            .stdout(stdout.try_clone().unwrap())
            .stderr(stderr.try_clone().unwrap())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < deadline, "clink did not finish: {args:?}");
        thread::sleep(Duration::from_millis(5));
    };
    Reply {
        status,
        stdout: read(stdout),
        stderr: read(stderr),
    }
}

#[test]
fn help_and_version_are_terminal_without_creating_stores() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("unused.links");
    assert_eq!(
        run(&db, &["--help"]).success().trim(),
        Cli::help_text().trim()
    );
    assert_eq!(
        run(&db, &["--version"]).success().trim(),
        Cli::version_text()
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn explicit_commit_modes_preserve_commits_across_reopening() {
    for mode in ["sync", " SyNc ", "async", " AsYnC ", "", "  "] {
        let dir = tempdir().unwrap();
        let db = dir.path().join("data.links");
        run(&db, &["--commit-mode", mode, "() ((1: 1 1))"]).success();
        assert_eq!(
            run(&db, &["--transactions", "--after"]).success(),
            "(1: 1 1)\n"
        );
        assert!(dir.path().join("data.transitions.links").exists());
    }
}

#[test]
fn empty_retention_uses_default_and_invalid_policies_do_not_create_stores() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("data.links");
    for args in [["--commit-mode", "bad"], ["--retention", "bad"]] {
        run(&db, &args).failure(args[0]);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
    run(&db, &["--retention", "  ", "() ((1: 1 1))"]).success();
    assert_eq!(run(&db, &["--after"]).success(), "(1: 1 1)\n");
}

#[test]
fn delimiter_tag_names_preserve_existing_alias_syntax_and_following_edits() {
    for name in ["release=雪=part", "release=0"] {
        let dir = tempdir().unwrap();
        let db = dir.path().join("data.links");
        run(&db, &["--vc", "() ((1 1))"]).success();
        run(&db, &["--tag", "first"]).success();
        let listing = run(&db, &["--list-tags"]).success();
        let (_, head) = listing.trim().split_once('\t').unwrap();
        let specification = format!("{name}={head}");
        run(&db, &["--tag", &specification]).success();
        let listing = run(&db, &["--list-tags"]).success();
        assert!(listing
            .lines()
            .any(|line| line == format!("{name}\t{head}")));

        // A complete existing tag has priority over a later numeric suffix.
        // A last-delimiter-only parser would break this previously valid alias.
        run(&db, &["--tag", &format!("alias={name}")]).success();
        let listing = run(&db, &["--list-tags"]).success();
        assert!(listing.lines().any(|line| line == format!("alias\t{head}")));
        assert!(!listing
            .lines()
            .any(|line| line.starts_with("alias=release\t")));
        run(&db, &["--vc", "() ((2 2))"]).success();
        for point in [name, "alias"] {
            assert_eq!(
                run(&db, &["--checkout", point, "--after"]).success(),
                "(1: 1 1)\n"
            );
        }
        run(&db, &["--tag", "new=still=missing"]).failure("Unknown tag point 'still=missing'.");
        run(&db, &["--tag", "new="]).failure("Unknown tag point ''.");
        assert_eq!(run(&db, &["--list-tags"]).success(), listing);
        run(&db, &["--vc", "() ((2 2))"]).success();
        assert_eq!(run(&db, &["--after"]).success(), "(1: 1 1)\n(2: 2 2)\n");
    }
}

#[test]
fn before_changes_and_after_render_the_same_persisted_transition() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("data.links");
    run(&db, &["() ((1: 1 1) (2: 2 2))"]).success();
    assert_eq!(
        run(
            &db,
            &[
                "--transactions",
                "--before",
                "--changes",
                "--after",
                "((1: 1 1)) ((1: 1 2))"
            ]
        )
        .success(),
        "(1: 1 1)\n(2: 2 2)\n((1: 1 1)) ((1: 1 2))\n(1: 1 2)\n(2: 2 2)\n"
    );
    assert_eq!(run(&db, &["--changes", ""]).success(), "");
    // The existing CLI reports an explicit identity substitution as a change.
    assert_eq!(
        run(&db, &["--changes", "((1: 1 2)) ((1: 1 2))"]).success(),
        "((1: 1 2)) ((1: 1 2))\n"
    );
    assert_eq!(run(&db, &["--after"]).success(), "(1: 1 2)\n(2: 2 2)\n");
}

#[test]
fn checkout_numeric_tag_and_alias_reopen_the_selected_history() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("data.links");
    run(&db, &["--vc", "() ((1: 1 1))"]).success();
    let output = run(&db, &["--tag", "origin", "--trace"]).success();
    assert!(output.contains("Tagged 'origin' at seq "), "{output}");
    let tags = run(&db, &["--list-tags"]).success();
    let seq = tags.strip_prefix("origin\t").unwrap().trim().to_owned();
    run(&db, &["--vc", "() ((2: 2 2))"]).success();
    let output = run(&db, &["--checkout", &seq, "--trace", "--after"]).success();
    assert!(
        output.contains(&format!("Checked out seq {seq} on branch 'main'.")),
        "{output}"
    );
    assert!(output.ends_with("(1: 1 1)\n"), "{output}");
    assert_eq!(run(&db, &["--vc", "--after"]).success(), "(1: 1 1)\n");
    run(&db, &["--checkout", "0", "--tag", "alias=origin"]).success();
    assert_eq!(run(&db, &["--vc", "--after"]).success(), "");
    assert_eq!(
        run(&db, &["--checkout", " alias ", "--after"]).success(),
        "(1: 1 1)\n"
    );
    run(&db, &["--tag", &format!("number={seq}"), "--trace"]).success();
    let tags = run(&db, &["--list-tags"]).success();
    assert!(tags.contains(&format!("alias\t{seq}\n")), "{tags}");
    assert!(tags.contains(&format!("number\t{seq}\n")), "{tags}");
}

#[test]
fn rejected_checkout_and_tag_leave_persistent_state_unchanged() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("data.links");
    run(&db, &["--vc", "() ((1: 1 1))"]).success();
    for (args, message) in [
        (["--checkout", "missing"], "Unknown checkout point"),
        (["--checkout", "  "], "Unknown checkout point"),
        (["--checkout", "999999"], "not reachable"),
        (["--checkout", "-1"], "non-negative"),
        (["--tag", "alias=missing"], "Unknown tag point"),
        (["--tag", "alias="], "Unknown tag point"),
        (["--tag", "=0"], "Tag name must not be empty"),
        (["--tag", "bad=-1"], "non-negative"),
    ] {
        run(&db, &args).failure(message);
        assert_eq!(run(&db, &["--vc", "--after"]).success(), "(1: 1 1)\n");
        assert_eq!(run(&db, &["--list-tags"]).success(), "");
    }
}

#[test]
fn tracing_new_and_existing_branches_preserves_distinct_data_and_log() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("data.links");
    run(&db, &["--vc", "() ((1: 1 1))"]).success();
    let created = run(&db, &["--branch", "topic", "--trace", "() ((2: 2 2))"]).success();
    assert!(created.contains("Created branch 'topic'."), "{created}");
    assert!(created.contains("Switched to branch 'topic'."), "{created}");
    let switched = run(&db, &["--branch", "main", "--trace", "--after"]).success();
    assert!(!switched.contains("Created branch 'main'."), "{switched}");
    assert!(switched.ends_with("(1: 1 1)\n"), "{switched}");
    let branches = run(&db, &["--list-branches"]).success();
    assert!(branches.contains("* main\tparent=-"), "{branches}");
    assert!(branches.contains("  topic\tparent=main"), "{branches}");
    let log = run(&db, &["--vc", "--log"]).success();
    assert!(log.contains("\tCreate\t"), "{log}");
    assert_eq!(
        run(&db, &["--branch", "topic", "--after"]).success(),
        "(1: 1 1)\n(2: 2 2)\n"
    );
}

#[test]
fn client_configuration_errors_precede_connections_and_store_creation() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("unused.links");
    for (args, message) in [
        (
            vec!["--serve", "127.0.0.1:1", "--connect", "127.0.0.1:1"],
            "cannot be used together",
        ),
        (
            vec!["--connect", "127.0.0.1:1", "--indexed-lookups"],
            "applies to local stores",
        ),
        (
            vec!["--connect", "127.0.0.1:1", "--protocol", "invalid"],
            "Invalid --protocol value",
        ),
        (
            vec![
                "--connect",
                "127.0.0.1:1",
                "--protocol",
                "text",
                "--external-references",
            ],
            "require --protocol binary",
        ),
    ] {
        run(&db, &args).failure(message);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}

#[test]
fn configured_clients_handle_empty_replies_without_opening_a_local_store() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("unused.links");
    let server = RunningServer::start(ServerOptions::default(), false);
    let address = server.address.to_string();
    for options in [
        vec![],
        vec!["--protocol", " TeXt "],
        vec!["--protocol", " BiNaRy "],
        vec!["--protocol", "binary", "--arity", "1..", "--packed-widths"],
        vec!["--external-references"],
    ] {
        let mut args = vec!["--connect", &address];
        args.extend(options);
        assert_eq!(run(&db, &args).success(), "");
        args.extend(["--query", "() ()"]);
        assert_eq!(run(&db, &args).success(), "");
    }
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn server_rejects_queries_and_invalid_policies_and_reports_bind_failure() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("served.links");
    for query in [vec!["() ((1: 1 1))"], vec!["--query", "() ((1: 1 1))"]] {
        let mut args = vec!["--serve", "127.0.0.1:0"];
        args.extend(query);
        run(&db, &args).failure("--serve does not take a query");
    }
    run(&db, &["--serve", "127.0.0.1:0", "--protocol", "invalid"])
        .failure("Invalid --protocol value");
    let occupied = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = occupied.local_addr().unwrap().to_string();
    for protocol in ["any", " TeXt ", " BINARY "] {
        run(&db, &["--serve", &address, "--protocol", protocol]).failure("Error:");
    }
    run(&db, &["--serve", &address]).failure("Error:");
    run(&db, &["--triggers", "--serve", &address]).failure("Error:");
    assert_eq!(run(&db, &["--after"]).success(), "");
}
