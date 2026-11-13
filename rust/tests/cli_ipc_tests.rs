//! `clink --serve`, `--connect` and `--forward` over IPC endpoints end to
//! end (issue #140): several endpoints on one store, relays stacking
//! transports, and a server on standard input and output.

#[path = "common/stop_server.rs"]
mod stop_server;

use link_cli::cli::{Cli, CliCommand};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, ChildStdout, Command, Output, Stdio};
use tempfile::TempDir;

const CLINK: &str = env!("CARGO_BIN_EXE_clink");

fn parse_run(args: &[&str]) -> Cli {
    match Cli::parse_from(args).expect("CLI arguments should parse") {
        CliCommand::Run(cli) => *cli,
        other => panic!("expected run command, got {other:?}"),
    }
}

fn endpoint(kind: &str, directory: &TempDir, name: &str) -> String {
    format!("{kind}:{}", directory.path().join(name).display())
}

/// A `clink` process serving until dropped, and the endpoints it printed.
struct Served {
    child: Child,
    endpoints: Vec<String>,
}

impl Served {
    fn start(args: &[&str]) -> Self {
        let mut child = Command::new(CLINK)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let count = args.iter().filter(|arg| **arg == "--serve").count();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let endpoints = (0..count).map(|_| banner(&mut stdout)).collect();
        Self { child, endpoints }
    }
}

fn banner(stdout: &mut BufReader<ChildStdout>) -> String {
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    line.trim()
        .strip_prefix("clink server listening on ")
        .unwrap_or_else(|| panic!("unexpected banner {line:?}"))
        .to_string()
}

impl Drop for Served {
    fn drop(&mut self) {
        stop_server::terminate(&mut self.child);
    }
}

fn connect(endpoint: &str, args: &[&str]) -> Output {
    Command::new(CLINK)
        .args(["--connect", endpoint])
        .args(args)
        .output()
        .unwrap()
}

fn stdout(endpoint: &str, args: &[&str]) -> String {
    let output = connect(endpoint, args);
    assert!(
        output.status.success(),
        "{endpoint} {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn database(directory: &TempDir) -> String {
    directory.path().join("served.links").display().to_string()
}

#[test]
fn parses_repeated_serve_and_forward() {
    let cli = parse_run(&[
        "clink",
        "--serve",
        "127.0.0.1:0",
        "--serve=shm:/tmp/a",
        "--serve",
        "unix:/tmp/b.sock",
        "--forward=mq:/tmp/c",
    ]);
    assert_eq!(cli.serve.as_deref(), Some("127.0.0.1:0"));
    assert_eq!(cli.serve_also, ["shm:/tmp/a", "unix:/tmp/b.sock"]);
    assert_eq!(
        cli.serve_endpoints(),
        ["127.0.0.1:0", "shm:/tmp/a", "unix:/tmp/b.sock"]
    );
    assert_eq!(cli.forward.as_deref(), Some("mq:/tmp/c"));
    assert!(parse_run(&["clink"]).serve_endpoints().is_empty());
    let help = Cli::help_text();
    assert!(help.contains("--forward <ADDR>"), "{help}");
    assert!(help.contains("shm:DIR"), "{help}");
    assert_eq!(
        Cli::parse_from(["clink", "--forward"])
            .unwrap_err()
            .to_string(),
        "missing value for option '--forward'"
    );
}

#[test]
fn one_server_serves_one_store_on_several_endpoints() {
    let directory = tempfile::tempdir().unwrap();
    let shm = endpoint("shm", &directory, "shm");
    let file = endpoint("file", &directory, "file");
    let db = database(&directory);
    let server = Served::start(&[
        "--db",
        &db,
        "--serve",
        "127.0.0.1:0",
        "--serve",
        &shm,
        "--serve",
        &file,
    ]);
    assert_eq!(server.endpoints[1..], [shm.clone(), file.clone()]);
    assert!(server.endpoints[0].starts_with("127.0.0.1:"));

    assert_eq!(stdout(&shm, &["() ((1 1))"]), "() ((1: 1 1))\n");
    assert_eq!(
        stdout(&file, &["--protocol", "binary", "() ((2 2))"]),
        "() ((2: 2 2))\n"
    );
    for endpoint in &server.endpoints {
        assert_eq!(
            stdout(endpoint, &[""]),
            "(1: 1 1)\n(2: 2 2)\n",
            "{endpoint}"
        );
    }
}

#[test]
fn relays_stack_transports_in_both_protocols() {
    let directory = tempfile::tempdir().unwrap();
    let db = database(&directory);
    let mmap = endpoint("mmap", &directory, "mmap");
    let shm = endpoint("shm", &directory, "shm");
    let _server = Served::start(&["--db", &db, "--serve", "127.0.0.1:0", "--serve", &mmap]);
    // shm -> mmap -> server.
    let relay = Served::start(&["--serve", &shm, "--forward", &mmap]);
    assert_eq!(relay.endpoints, std::slice::from_ref(&shm));
    assert_eq!(stdout(&shm, &["() ((1 1))"]), "() ((1: 1 1))\n");
    assert_eq!(stdout(&shm, &["--protocol", "binary", ""]), "(1: 1 1)\n");
    // exec -> stdio -> shm -> mmap -> server.
    let command = format!(
        "exec:[\"{}\",\"--serve\",\"stdio\",\"--forward\",\"{}\"]",
        json(CLINK),
        json(&shm)
    );
    assert_eq!(stdout(&command, &[""]), "(1: 1 1)\n");
}

#[test]
fn a_stdio_server_serves_one_connection_and_saves() {
    let directory = tempfile::tempdir().unwrap();
    let db = database(&directory);
    let command = format!(
        "exec:[\"{}\",\"--db\",\"{}\",\"--serve\",\"stdio\"]",
        json(CLINK),
        json(&db)
    );
    assert_eq!(stdout(&command, &["() ((1 1))"]), "() ((1: 1 1))\n");
    // The store was saved when the only connection ended; a second
    // server process reads it back.
    assert!(Path::new(&db).exists());
    assert_eq!(stdout(&command, &[""]), "(1: 1 1)\n");
}

#[test]
fn misused_forward_and_graphql_are_refused() {
    let directory = tempfile::tempdir().unwrap();
    let shm = endpoint("shm", &directory, "shm");
    let refused = |args: &[&str], message: &str| {
        let output = Command::new(CLINK).args(args).output().unwrap();
        assert!(!output.status.success(), "{args:?}");
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains(message), "{args:?}: {error}");
    };
    refused(&["--forward", &shm], "give --serve as well");
    refused(
        &["--serve", &shm, "--forward", "127.0.0.1:1", "() ((1 1))"],
        "does not take a query",
    );
    refused(&["--serve", &shm, "--graphql"], "over TCP only");
    refused(
        &["--serve", "127.0.0.1:0", "--serve", &shm, "--graphql"],
        "give --serve once",
    );
    let output = connect(&endpoint("shm", &directory, "nobody"), &[""]);
    assert!(!output.status.success());
}

/// Escapes `text` for a JSON string.
fn json(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}
