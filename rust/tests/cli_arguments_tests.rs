//! Tests for Rust CLI argument parity with the C# command surface.

use link_cli::cli::{Cli, CliCommand};

fn parse_run(args: &[&str]) -> Cli {
    match Cli::parse_from(args).expect("CLI arguments should parse") {
        CliCommand::Run(cli) => cli,
        other => panic!("expected run command, got {other:?}"),
    }
}

#[test]
fn parses_csharp_option_aliases_without_direct_clap_dependency() {
    let cli = parse_run(&[
        "clink",
        "--data-source",
        "links.db",
        "--apply",
        "(1 2)",
        "--links",
        "--out",
        "dump.lino",
        "-b",
        "-c",
        "-t",
        "-s",
        "42",
    ]);

    assert_eq!(cli.db, "links.db");
    assert_eq!(cli.query.as_deref(), Some("(1 2)"));
    assert!(cli.after);
    assert!(cli.before);
    assert!(cli.changes);
    assert!(cli.trace);
    assert_eq!(cli.structure, Some(42));
    assert_eq!(cli.lino_output.as_deref(), Some("dump.lino"));
}

#[test]
fn query_option_takes_precedence_over_positional_query() {
    let cli = parse_run(&["clink", "--query", "(1 2)", "(3 4)"]);

    assert_eq!(cli.query.as_deref(), Some("(1 2)"));
    assert_eq!(cli.query_arg.as_deref(), Some("(3 4)"));
}

#[test]
fn parses_inline_alias_values_and_boolean_values() {
    let cli = parse_run(&[
        "clink",
        "--data=db.bin",
        "--do=(5 6)",
        "--trace=false",
        "--before=true",
        "--changes=on",
        "--after=0",
        "--lino-output=links.lino",
    ]);

    assert_eq!(cli.db, "db.bin");
    assert_eq!(cli.query.as_deref(), Some("(5 6)"));
    assert!(!cli.trace);
    assert!(cli.before);
    assert!(cli.changes);
    assert!(!cli.after);
    assert_eq!(cli.lino_output.as_deref(), Some("links.lino"));
}

#[test]
fn returns_help_and_version_commands() {
    assert_eq!(
        Cli::parse_from(["clink", "--help"]).expect("help should parse"),
        CliCommand::Help
    );
    assert_eq!(
        Cli::parse_from(["clink", "--version"]).expect("version should parse"),
        CliCommand::Version
    );
}

#[test]
fn rejects_extra_positional_queries() {
    let error = Cli::parse_from(["clink", "(1 2)", "(3 4)"]).expect_err("extra query should fail");

    assert!(error.to_string().contains("unexpected extra positional"));
}
