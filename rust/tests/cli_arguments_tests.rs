//! Tests for Rust CLI argument parity with the C# command surface.

use link_cli::cli::{Cli, CliCommand};

fn parse_run(args: &[&str]) -> Cli {
    match Cli::parse_from(args).expect("CLI arguments should parse") {
        CliCommand::Run(cli) => *cli,
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
        "--import",
        "input.lino",
        "--out",
        "dump.lino",
        "-b",
        "-c",
        "-t",
        "--auto-create-missing-references",
        "-s",
        "42",
    ]);

    assert_eq!(cli.db, "links.db");
    assert_eq!(cli.query.as_deref(), Some("(1 2)"));
    assert!(cli.after);
    assert!(cli.before);
    assert!(cli.changes);
    assert!(cli.trace);
    assert!(cli.auto_create_missing_references);
    assert_eq!(cli.structure, Some(42));
    assert_eq!(cli.lino_input.as_deref(), Some("input.lino"));
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
        "--auto-create-missing-references=true",
        "--before=true",
        "--changes=on",
        "--after=0",
        "--lino-input=input.lino",
        "--lino-output=links.lino",
    ]);

    assert_eq!(cli.db, "db.bin");
    assert_eq!(cli.query.as_deref(), Some("(5 6)"));
    assert!(!cli.trace);
    assert!(cli.auto_create_missing_references);
    assert!(cli.before);
    assert!(cli.changes);
    assert!(!cli.after);
    assert_eq!(cli.lino_input.as_deref(), Some("input.lino"));
    assert_eq!(cli.lino_output.as_deref(), Some("links.lino"));
}

#[test]
fn parses_export_alias_as_lino_output_path() {
    let cli = parse_run(&["clink", "--export", "database.lino"]);

    assert_eq!(cli.lino_output.as_deref(), Some("database.lino"));
}

#[test]
fn parses_inline_export_alias_as_lino_output_path() {
    let cli = parse_run(&["clink", "--export=database.lino"]);

    assert_eq!(cli.lino_output.as_deref(), Some("database.lino"));
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

#[test]
fn parses_transactions_flag_family() {
    let cli = parse_run(&[
        "clink",
        "--transactions",
        "--transactions-file",
        "trans.links",
        "--commit-mode",
        "async",
        "--retention",
        "sized:128",
        "--log",
    ]);

    assert!(cli.transactions);
    assert_eq!(cli.transactions_file.as_deref(), Some("trans.links"));
    assert_eq!(cli.commit_mode.as_deref(), Some("async"));
    assert_eq!(cli.retention.as_deref(), Some("sized:128"));
    assert!(cli.show_log);
    assert!(cli.transactions_requested());
    assert!(!cli.vc_requested());
}

#[test]
fn parses_inline_transactions_flag_values() {
    let cli = parse_run(&[
        "clink",
        "--transactions=true",
        "--transactions-file=tx.links",
        "--commit-mode=sync",
        "--retention=chunked:64:/tmp/archive",
        "--log=true",
    ]);

    assert!(cli.transactions);
    assert_eq!(cli.transactions_file.as_deref(), Some("tx.links"));
    assert_eq!(cli.commit_mode.as_deref(), Some("sync"));
    assert_eq!(cli.retention.as_deref(), Some("chunked:64:/tmp/archive"));
    assert!(cli.show_log);
}

#[test]
fn parses_version_control_flag_family() {
    let cli = parse_run(&[
        "clink",
        "--vc",
        "--vc-file",
        "vc.links",
        "--branch",
        "feature",
        "--branch-from",
        "3",
        "--checkout",
        "main",
        "--tag",
        "release=2",
        "--list-branches",
        "--list-tags",
    ]);

    assert!(cli.vc);
    assert_eq!(cli.vc_file.as_deref(), Some("vc.links"));
    assert_eq!(cli.branch.as_deref(), Some("feature"));
    assert_eq!(cli.branch_from, Some(3));
    assert_eq!(cli.checkout.as_deref(), Some("main"));
    assert_eq!(cli.tag.as_deref(), Some("release=2"));
    assert!(cli.list_branches);
    assert!(cli.list_tags);
    assert!(cli.vc_requested());
    // version-control implies transactions.
    assert!(cli.transactions_requested());
}

#[test]
fn parses_inline_version_control_flag_values() {
    let cli = parse_run(&[
        "clink",
        "--vc=true",
        "--vc-file=vc.bin",
        "--branch=topic",
        "--branch-from=7",
        "--checkout=v1.0",
        "--tag=v2.0",
        "--list-branches=false",
        "--list-tags=true",
    ]);

    assert!(cli.vc);
    assert_eq!(cli.vc_file.as_deref(), Some("vc.bin"));
    assert_eq!(cli.branch.as_deref(), Some("topic"));
    assert_eq!(cli.branch_from, Some(7));
    assert_eq!(cli.checkout.as_deref(), Some("v1.0"));
    assert_eq!(cli.tag.as_deref(), Some("v2.0"));
    assert!(!cli.list_branches);
    assert!(cli.list_tags);
}

#[test]
fn defaults_have_no_transactions_or_vc_requested() {
    let cli = parse_run(&["clink"]);

    assert!(!cli.transactions_requested());
    assert!(!cli.vc_requested());
    assert!(!cli.transactions);
    assert!(!cli.vc);
}

#[test]
fn rejects_invalid_branch_from_value() {
    let error = Cli::parse_from(["clink", "--branch-from", "not-a-number"])
        .expect_err("invalid branch-from should fail");
    assert!(
        error.to_string().contains("invalid sequence value"),
        "unexpected error message: {error}"
    );
}
