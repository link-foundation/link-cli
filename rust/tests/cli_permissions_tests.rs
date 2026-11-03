//! `clink --permissions`, `--subject` and the token table end to end (issue
//! #149). The C# counterpart is `CliPermissionsTests`.

#[path = "common/permissions_world.rs"]
mod world;

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};

use link_cli::cli::{Cli, CliCommand};
use link_cli::{NamedTypeLinks, NamedTypesDecorator, PermissionPolicy, PermissionsDecorator};
use tempfile::TempDir;
use world::{make_world, Names};

struct Fixture {
    directory: TempDir,
}

struct Made {
    t: Names,
    v: Names,
    alice_sees: Vec<String>,
    all: Vec<String>,
}

impl Fixture {
    fn new() -> Self {
        Self {
            directory: TempDir::new().unwrap(),
        }
    }

    fn data(&self) -> PathBuf {
        self.directory.path().join("data.links")
    }

    fn rules(&self) -> PathBuf {
        self.directory.path().join("rules.links")
    }

    fn path(&self, name: &str) -> String {
        self.directory.path().join(name).display().to_string()
    }

    /// Writes the world of the permission tests into the data, its rules
    /// there or in a file of their own.
    fn make_world(&self, separate_rules: bool) -> Made {
        let mut data = NamedTypesDecorator::new(self.data(), false).unwrap();
        let mut rules = NamedTypesDecorator::new(self.rules(), false).unwrap();
        let (t, v, _, _) = make_world(
            &mut data,
            if separate_rules {
                Some(&mut rules)
            } else {
                None
            },
        )
        .unwrap();
        rules.save().unwrap();
        let policy = if separate_rules {
            PermissionPolicy::with_rules(&mut data, Box::new(rules)).unwrap()
        } else {
            PermissionPolicy::in_data(&mut data).unwrap()
        };
        let alice_sees = PermissionsDecorator::new(&mut data, policy, v["alice"])
            .lino_lines()
            .unwrap();
        let all = data.lino_lines().unwrap();
        data.save().unwrap();
        Made {
            t,
            v,
            alice_sees,
            all,
        }
    }

    fn clink(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_clink"))
            .args(args)
            .output()
            .unwrap()
    }

    fn on_data(&self, args: &[&str]) -> Output {
        let data = self.data().display().to_string();
        let mut all = vec!["--db", data.as_str()];
        all.extend_from_slice(args);
        self.clink(&all)
    }

    fn permitted(&self, separate_rules: bool, args: &[&str]) -> Output {
        let rules = if separate_rules {
            self.rules().display().to_string()
        } else {
            "self".to_string()
        };
        let mut all = vec!["--permissions", rules.as_str()];
        all.extend_from_slice(args);
        self.on_data(&all)
    }

    fn after(&self) -> Vec<String> {
        lines(&succeeded(self.on_data(&["--after"])))
    }

    fn serve(&self, extra: &[&str]) -> Server {
        Server::start(&self.data(), extra)
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn succeeded(output: Output) -> String {
    assert!(output.status.success(), "clink failed: {}", stderr(&output));
    stdout(&output)
}

fn lines(stdout: &str) -> Vec<String> {
    stdout.lines().map(str::to_string).collect()
}

fn failed_with(output: &Output, message: &str) {
    assert_eq!(output.status.code(), Some(1), "{}", stdout(output));
    assert!(
        stderr(output).contains(message),
        "{message:?} not in {:?}",
        stderr(output)
    );
}

fn a_subject_sees_what_its_rules_allow_and_writes_nothing_else(separate_rules: bool) {
    let fixture = Fixture::new();
    let Made {
        t,
        v,
        alice_sees,
        all,
    } = fixture.make_world(separate_rules);
    // Without --permissions nothing changes: every link is there.
    assert_eq!(fixture.after(), all);

    let seen = succeeded(fixture.permitted(separate_rules, &["--subject", "alice", "--after"]));
    assert_eq!(lines(&seen), alice_sees);
    assert!(alice_sees.contains(&"(note: Note note)".to_string()));
    assert!(alice_sees.contains(&"(shared: Shared shared)".to_string()));
    assert!(!alice_sees.iter().any(|line| line.contains("secret")));

    // A note of alice's may become another note, not a secret.
    let refused = fixture.permitted(
        separate_rules,
        &[
            "--subject",
            "alice",
            "--changes",
            "((note: Note note)) ((note: Secret note))",
        ],
    );
    failed_with(&refused, &format!("Forbidden: update {}", v["note"]));
    assert_eq!(fixture.after(), all);

    // A secret is as missing to alice as a deleted link.
    let alice = v["alice"].to_string();
    let hidden = succeeded(fixture.permitted(
        separate_rules,
        &["--subject", &alice, "--changes", "((secret: * *)) ()"],
    ));
    assert_eq!(hidden, "");
    assert_eq!(fixture.after(), all);

    // The boss writes anything.
    let boss = succeeded(fixture.permitted(
        separate_rules,
        &[
            "--subject",
            "boss",
            "--changes",
            "((note: Note note)) ((note: Secret note))",
        ],
    ));
    assert!(
        boss.contains("((note: Note note)) ((note: Secret note))"),
        "{boss}"
    );
    assert_ne!(t["Secret"], 0);
}

#[test]
fn a_subject_sees_what_its_rules_allow_and_writes_nothing_else_with_rules_in_the_data() {
    a_subject_sees_what_its_rules_allow_and_writes_nothing_else(false);
}

#[test]
fn a_subject_sees_what_its_rules_allow_and_writes_nothing_else_with_rules_apart() {
    a_subject_sees_what_its_rules_allow_and_writes_nothing_else(true);
}

#[test]
fn a_guest_reads_nothing_and_creates_nothing() {
    let fixture = Fixture::new();
    fixture.make_world(false);
    let seen = succeeded(fixture.permitted(false, &["--after"]));
    assert_eq!(seen, "");
    failed_with(
        &fixture.permitted(false, &["() ((1 2))"]),
        "Forbidden: create",
    );
    failed_with(
        &fixture.permitted(false, &["--subject", "nobody", "--after"]),
        "Unknown subject 'nobody'",
    );
}

#[test]
fn token_commands_need_a_token_table_and_a_subject() {
    let fixture = Fixture::new();
    fixture.make_world(false);
    failed_with(
        &fixture.on_data(&["--subject", "alice", "--grant-token", "x"]),
        "--grant-token and --revoke-token need --tokens.",
    );
    failed_with(
        &fixture.on_data(&["--tokens", "self", "--grant-token", "x"]),
        "--grant-token needs a --subject.",
    );
    assert_eq!(
        succeeded(fixture.on_data(&["--tokens", "self", "--revoke-token", "x"])),
        "Unknown token.\n"
    );
    failed_with(
        &fixture.permitted(false, &["--always", "() ((1 1))"]),
        "--always, --once and --never cannot be used with --permissions.",
    );
}

fn served_connections_authenticate_with_tokens_stored_as_links(separate_tokens: bool) {
    let fixture = Fixture::new();
    let Made {
        v,
        alice_sees,
        mut all,
        ..
    } = fixture.make_world(false);
    let tokens = if separate_tokens {
        fixture.path("tokens.links")
    } else {
        "self".to_string()
    };
    let granted = succeeded(fixture.on_data(&[
        "--tokens",
        &tokens,
        "--subject",
        "alice",
        "--grant-token",
        "alice-secret",
    ]));
    assert!(
        granted.starts_with(&format!("Token granted to subject {}", v["alice"])),
        "{granted}"
    );
    if !separate_tokens {
        // The token links are links of the data, which alice cannot see.
        all = fixture.after();
    }

    {
        let server = fixture.serve(&["--permissions", "self", "--tokens", &tokens]);
        assert_eq!(succeeded(server.connect(&["(count: ())"])), "(count: 0)\n");
        assert_eq!(
            succeeded(server.connect(&["--token", "alice-secret", "(count: ())"])),
            format!("(count: {})\n", alice_sees.len())
        );
        let update = format!("(update: ({} {} {}))", v["note"], v["secret"], v["note"]);
        let refused = server.connect(&["--token", "alice-secret", &update]);
        assert!(!refused.status.success(), "{}", stdout(&refused));
        let wrong = server.connect(&["--token", "wrong", "(count: ())"]);
        assert!(!wrong.status.success());
        assert!(
            stderr(&wrong).contains("Forbidden: authentication failed"),
            "{}",
            stderr(&wrong)
        );
    }

    assert_eq!(fixture.after(), all);
    let revoked =
        succeeded(fixture.on_data(&["--tokens", &tokens, "--revoke-token", "alice-secret"]));
    assert_eq!(revoked, "Token revoked.\n");
}

#[test]
fn served_connections_authenticate_with_tokens_stored_as_links_in_the_data() {
    served_connections_authenticate_with_tokens_stored_as_links(false);
}

#[test]
fn served_connections_authenticate_with_tokens_stored_as_links_apart() {
    served_connections_authenticate_with_tokens_stored_as_links(true);
}

#[test]
fn parses_permission_options() {
    let cli = match Cli::parse_from([
        "clink",
        "--permissions",
        "rules.links",
        "--subject=alice",
        "--tokens",
        "self",
        "--grant-token=a",
        "--revoke-token",
        "b",
        "--token=c",
    ])
    .unwrap()
    {
        CliCommand::Run(cli) => *cli,
        other => panic!("expected run command, got {other:?}"),
    };
    assert_eq!(cli.permissions.as_deref(), Some("rules.links"));
    assert_eq!(cli.subject.as_deref(), Some("alice"));
    assert_eq!(cli.tokens.as_deref(), Some("self"));
    assert_eq!(cli.grant_token.as_deref(), Some("a"));
    assert_eq!(cli.revoke_token.as_deref(), Some("b"));
    assert_eq!(cli.token.as_deref(), Some("c"));
    for option in [
        "--permissions <RULES>",
        "--subject <LINK>",
        "--tokens <TOKENS>",
        "--grant-token <TOKEN>",
        "--revoke-token <TOKEN>",
        "--token <TOKEN>",
    ] {
        assert!(Cli::help_text().contains(option), "{option}");
    }
}

struct Server {
    child: Child,
    address: String,
}

impl Server {
    fn start(database: &Path, extra: &[&str]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_clink"))
            .arg("--db")
            .arg(database)
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
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
