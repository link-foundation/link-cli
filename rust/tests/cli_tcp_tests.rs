//! `clink --serve` and `clink --connect` end to end (issue #105).

use link_cli::cli::{Cli, CliCommand};
use link_cli::protocol::ArityRange;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Output, Stdio};
use tempfile::TempDir;

fn parse_run(args: &[&str]) -> Cli {
    match Cli::parse_from(args).expect("CLI arguments should parse") {
        CliCommand::Run(cli) => *cli,
        other => panic!("expected run command, got {other:?}"),
    }
}

#[test]
fn parses_tcp_options() {
    let cli = parse_run(&[
        "clink",
        "--serve",
        "127.0.0.1:7878",
        "--protocol=binary",
        "--external-references",
        "--arity",
        "2..3",
        "--packed-widths=on",
    ]);
    assert_eq!(cli.serve.as_deref(), Some("127.0.0.1:7878"));
    assert_eq!(cli.protocol.as_deref(), Some("binary"));
    assert!(cli.external_references);
    assert_eq!(cli.arity, ArityRange::between(2, 3));
    assert!(cli.packed_widths);

    let cli = parse_run(&["clink", "--connect=localhost:1", "--arity=1.."]);
    assert_eq!(cli.arity, ArityRange::at_least(1));

    let cli = parse_run(&["clink", "--connect=localhost:1", "() ((1 1))"]);
    assert_eq!(cli.connect.as_deref(), Some("localhost:1"));
    assert_eq!(cli.query_arg.as_deref(), Some("() ((1 1))"));
    assert!(!cli.external_references && !cli.packed_widths);
    assert_eq!(cli.arity, ArityRange::DOUBLETS);
    assert!(Cli::help_text().contains("--serve <ADDR>"));
    assert!(Cli::help_text().contains("--connect <ADDR>"));
}

struct Server {
    child: Child,
    address: String,
}

impl Server {
    fn start(directory: &TempDir, extra: &[&str]) -> Self {
        let database = directory.path().join("served.links");
        let mut child = Command::new(env!("CARGO_BIN_EXE_clink"))
            .arg("--db")
            .arg(&database)
            .args(["--serve", "127.0.0.1:0"])
            .args(extra)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let address = line
            .trim()
            .strip_prefix("clink server listening on ")
            .unwrap_or_else(|| panic!("unexpected banner {line:?}"))
            .to_string();
        Self { child, address }
    }

    fn connect(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_clink"))
            .args(["--connect", &self.address])
            .args(args)
            .output()
            .unwrap()
    }

    fn stdout(&self, args: &[&str]) -> String {
        let output = self.connect(args);
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn clients_query_a_served_database_over_both_protocols() {
    let directory = TempDir::new().unwrap();
    let server = Server::start(&directory, &[]);

    assert_eq!(server.stdout(&["() ((1 1))"]), "() ((1: 1 1))\n");
    assert_eq!(
        server.stdout(&["--protocol", "binary", "() ((2 2))"]),
        "() ((2: 2 2))\n"
    );
    assert_eq!(
        server.stdout(&[
            "--external-references",
            "--arity",
            "1..",
            "((1: 1 1)) ((1: 1 2))"
        ]),
        "((1: 1 1)) ((1: 1 2))\n"
    );
    assert_eq!(server.stdout(&[]), "(1: 1 2)\n(2: 2 2)\n");
    assert_eq!(
        server
            .stdout(&["--protocol", "binary", "((2: 2 2)) ()"])
            .lines()
            .count(),
        2
    );

    let failure = server.connect(&["((99: 1 1)) ()"]);
    assert!(!failure.status.success());
    assert!(String::from_utf8_lossy(&failure.stderr).contains("server error"));

    let conflicting = server.connect(&["--protocol", "text", "--arity", "2..3", ""]);
    assert!(!conflicting.status.success());
}

#[test]
fn served_changes_are_persisted() {
    let directory = TempDir::new().unwrap();
    {
        let server = Server::start(&directory, &["--auto-create-missing-references"]);
        assert_eq!(
            server
                .stdout(&["() ((child: father mother))"])
                .lines()
                .last(),
            Some("() ((child: father mother))")
        );
    }
    let output = Command::new(env!("CARGO_BIN_EXE_clink"))
        .arg("--db")
        .arg(directory.path().join("served.links"))
        .arg("--after")
        .output()
        .unwrap();
    let listing = String::from_utf8(output.stdout).unwrap();
    assert!(listing.contains("(child: father mother)"), "{listing}");
}

#[test]
fn serve_rejects_a_query() {
    let directory = TempDir::new().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_clink"))
        .arg("--db")
        .arg(directory.path().join("x.links"))
        .args(["--serve", "127.0.0.1:0", "() ((1 1))"])
        .output()
        .unwrap();
    assert!(!output.status.success());
}
