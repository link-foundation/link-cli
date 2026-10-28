//! Public configuration semantics: spelling parity, isolation and finite limits.

use link_cli::cli::{Cli, CliCommand};
use link_cli::protocol::ArityRange;

fn run(args: &[&str]) -> Cli {
    match Cli::parse_from(args).expect("valid public arguments") {
        CliCommand::Run(cli) => *cli,
        other => panic!("expected configuration, got {other:?}"),
    }
}

const VALUE: &str = "name=雪 part\nend";

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn string_option_aliases_preserve_complete_inline_and_separate_values() {
    macro_rules! string_cases {
        ($([$($alias:literal),+] => $field:ident),+ $(,)?) => {{
            $(
                let expected = Cli { $field: Some(VALUE.into()), ..Cli::default() };
                for alias in [$($alias),+] {
                    assert_eq!(run(&["clink", alias, VALUE]), expected, "{alias}");
                    assert_eq!(run(&["clink", &format!("{alias}={VALUE}")]), expected, "{alias}");
                    assert_eq!(run(&["clink", alias, ""]), Cli { $field: Some(String::new()), ..Cli::default() });
                }
            )+
        }};
    }
    string_cases! {
        ["--query", "--apply", "--do"] => query,
        ["--in", "--lino-input", "--import"] => lino_input,
        ["--out", "--lino-output", "--export"] => lino_output,
        ["--import-binary", "--binary-input", "--binary-in"] => binary_input,
        ["--export-binary", "--binary-output", "--binary-out"] => binary_output,
        ["--triggers-file"] => triggers_file,
        ["--transactions-file"] => transactions_file,
        ["--commit-mode"] => commit_mode,
        ["--retention"] => retention,
        ["--vc-file"] => vc_file,
        ["--branch"] => branch,
        ["--checkout"] => checkout,
        ["--tag"] => tag,
        ["--serve"] => serve,
        ["--connect"] => connect,
        ["--protocol"] => protocol,
    }
    for alias in ["-d", "--db", "--data-source", "--data"] {
        let expected = Cli {
            db: VALUE.into(),
            ..Cli::default()
        };
        assert_eq!(run(&["clink", alias, VALUE]), expected);
        if alias.starts_with("--") {
            assert_eq!(run(&["clink", &format!("{alias}={VALUE}")]), expected);
        }
    }
    assert_eq!(
        run(&["clink", "-q", VALUE]),
        Cli {
            query: Some(VALUE.into()),
            ..Cli::default()
        }
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn boolean_options_have_consistent_literals_and_leave_query_tokens_intact() {
    macro_rules! boolean_cases {
        ($([$($alias:literal),+] => $field:ident),+ $(,)?) => {{
            $(
                for alias in [$($alias),+] {
                    assert_eq!(run(&["clink", alias]), Cli { $field: true, ..Cli::default() });
                    for (literal, expected) in [
                        ("true", true), ("1", true), ("YeS", true), ("ON", true),
                        ("false", false), ("0", false), ("nO", false), ("OFF", false),
                    ] {
                        let expected = Cli { $field: expected, query_arg: Some("(1 1)".into()), ..Cli::default() };
                        assert_eq!(run(&["clink", alias, literal, "(1 1)"]), expected, "{alias} {literal}");
                        if alias.starts_with("--") {
                            assert_eq!(run(&["clink", &format!("{alias}={literal}"), "(1 1)"]), expected);
                        }
                    }
                    for query in ["(1 1)", "named=雪", "maybe"] {
                        assert_eq!(run(&["clink", alias, query]), Cli { $field: true, query_arg: Some(query.into()), ..Cli::default() });
                    }
                    if alias.starts_with("--") {
                        let diagnostic = [$($alias),+].into_iter().find(|name| name.starts_with("--")).unwrap();
                        for invalid in ["", "maybe", "2", " true "] {
                            let error = Cli::parse_from(["clink", &format!("{alias}={invalid}")]).unwrap_err();
                            assert_eq!(error.to_string(), format!("invalid boolean value '{invalid}' for {diagnostic}"));
                            assert_eq!(run(&["clink"]), Cli::default());
                        }
                    }
                }
            )+
        }};
    }
    boolean_cases! {
        ["-t", "--trace"] => trace,
        ["--indexed-lookups"] => indexed_lookups,
        ["--auto-create-missing-references"] => auto_create_missing_references,
        ["-b", "--before"] => before,
        ["-c", "--changes"] => changes,
        ["-a", "--after", "--links"] => after,
        ["--always"] => always,
        ["--once"] => once,
        ["--never"] => never,
        ["--triggers"] => triggers,
        ["--embed-triggers"] => embed_triggers,
        ["--transactions"] => transactions,
        ["--vc"] => vc,
        ["--list-branches"] => list_branches,
        ["--list-tags"] => list_tags,
        ["--log"] => show_log,
        ["--external-references"] => external_references,
        ["--packed-widths"] => packed_widths,
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn required_values_report_the_actual_option_and_a_later_parse_is_independent() {
    for option in [
        "-d",
        "--db",
        "--data-source",
        "--data",
        "-q",
        "--query",
        "--apply",
        "--do",
        "-s",
        "--structure",
        "--in",
        "--lino-input",
        "--import",
        "--out",
        "--lino-output",
        "--export",
        "--import-binary",
        "--binary-input",
        "--binary-in",
        "--export-binary",
        "--binary-output",
        "--binary-out",
        "--triggers-file",
        "--transactions-file",
        "--commit-mode",
        "--retention",
        "--vc-file",
        "--branch",
        "--branch-from",
        "--checkout",
        "--tag",
        "--serve",
        "--connect",
        "--protocol",
        "--arity",
    ] {
        let error = Cli::parse_from(["clink", option]).unwrap_err();
        assert_eq!(
            error.to_string(),
            format!("missing value for option '{option}'")
        );
        assert_eq!(run(&["clink", "--tag", VALUE]).tag.as_deref(), Some(VALUE));
        assert_eq!(run(&["clink"]), Cli::default());
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn numeric_boundaries_preserve_full_addresses_and_signed_sequence_points() {
    for option in ["-s", "--structure"] {
        for number in [0u32, 1, u32::MAX] {
            let value = number.to_string();
            let expected = Cli {
                structure: Some(number),
                ..Cli::default()
            };
            assert_eq!(run(&["clink", option, &value]), expected);
            if option.starts_with("--") {
                assert_eq!(run(&["clink", &format!("{option}={value}")]), expected);
            }
        }
        for invalid in ["", "-1", "4294967296", "1.0", "雪"] {
            let error = Cli::parse_from(["clink", option, invalid]).unwrap_err();
            assert!(error.to_string().contains("invalid link id"), "{error}");
            assert!(error.to_string().contains(option), "{error}");
        }
    }
    for number in [i64::MIN, -1, 0, 1, i64::MAX] {
        let value = number.to_string();
        let expected = Cli {
            branch_from: Some(number),
            ..Cli::default()
        };
        assert_eq!(run(&["clink", "--branch-from", &value]), expected);
        assert_eq!(run(&["clink", &format!("--branch-from={value}")]), expected);
    }
    for invalid in [
        "",
        "-9223372036854775809",
        "9223372036854775808",
        "1.0",
        "雪",
    ] {
        for args in [
            vec!["clink".into(), "--branch-from".into(), invalid.into()],
            vec!["clink".into(), format!("--branch-from={invalid}")],
        ] {
            let error = Cli::parse_from(args).unwrap_err();
            assert!(
                error.to_string().contains("invalid sequence value"),
                "{error}"
            );
        }
    }
    assert_eq!(run(&["clink"]), Cli::default());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn arity_ranges_validate_the_wire_shape_without_changing_other_defaults() {
    for (input, expected) in [
        ("1", ArityRange::exactly(1)),
        ("2", ArityRange::DOUBLETS),
        ("2..3", ArityRange::between(2, 3)),
        ("1..", ArityRange::at_least(1)),
        (" \t2..3\n", ArityRange::between(2, 3)),
    ] {
        let expected = Cli {
            arity: expected,
            ..Cli::default()
        };
        assert_eq!(run(&["clink", "--arity", input]), expected);
        assert_eq!(run(&["clink", &format!("--arity={input}")]), expected);
    }
    for invalid in [
        "",
        "0",
        "3..2",
        "..3",
        "2...3",
        "18446744073709551616",
        "雪",
    ] {
        for args in [
            vec!["clink".into(), "--arity".into(), invalid.into()],
            vec!["clink".into(), format!("--arity={invalid}")],
        ] {
            let error = Cli::parse_from(args).unwrap_err();
            assert!(
                error.to_string().contains("invalid value for '--arity'"),
                "{error}"
            );
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn literal_queries_option_boundaries_and_repeated_values_are_unambiguous() {
    for query in ["--trace", "--tag=雪", "--help", "--", "-", ""] {
        assert_eq!(
            run(&["clink", "--", query]),
            Cli {
                query_arg: Some(query.into()),
                ..Cli::default()
            }
        );
    }
    for unknown in [
        "--trace-extra",
        "--db-file",
        "--version=1",
        "--help=yes",
        "-z",
    ] {
        assert_eq!(
            Cli::parse_from(["clink", unknown]).unwrap_err().to_string(),
            format!("unknown option '{unknown}'")
        );
    }
    assert_eq!(run(&["clink", "--"]).query_arg, None);
    assert!(Cli::parse_from(["clink", "--", "first", "second"])
        .unwrap_err()
        .to_string()
        .contains("unexpected extra positional argument 'second'"));
    let cli = run(&[
        "clink",
        "--query",
        "first",
        "--do=雪=last",
        "--trace",
        "--trace=false",
        "--",
        "positional",
    ]);
    assert_eq!(
        cli,
        Cli {
            query: Some("雪=last".into()),
            query_arg: Some("positional".into()),
            ..Cli::default()
        }
    );
    for (option, command) in [
        ("-h", CliCommand::Help),
        ("--help", CliCommand::Help),
        ("-V", CliCommand::Version),
        ("--version", CliCommand::Version),
    ] {
        assert_eq!(
            Cli::parse_from(["clink", option, "--unknown"]).unwrap(),
            command
        );
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn strategy_families_are_selected_only_by_their_own_enabled_options() {
    for option in [
        "--trace",
        "--indexed-lookups",
        "--before",
        "--changes",
        "--after",
        "--external-references",
        "--packed-widths",
    ] {
        let cli = run(&["clink", option]);
        assert!(!cli.transactions_requested());
        assert!(!cli.vc_requested());
        assert!(!cli.persistent_transformations_requested());
        assert_eq!(cli.trigger_command_count(), 0);
    }
    for option in [
        "--transactions",
        "--log",
        "--vc",
        "--list-branches",
        "--list-tags",
        "--always",
        "--once",
        "--never",
        "--triggers",
        "--embed-triggers",
    ] {
        let cli = run(&["clink", &format!("{option}=false")]);
        assert!(!cli.transactions_requested());
        assert!(!cli.vc_requested());
        assert!(!cli.persistent_transformations_requested());
        assert_eq!(cli.trigger_command_count(), 0);
    }
    for option in ["--transactions-file", "--commit-mode", "--retention"] {
        let cli = run(&["clink", option, VALUE]);
        assert!(cli.transactions_requested());
        assert!(!cli.vc_requested());
        assert!(!cli.persistent_transformations_requested());
    }
    for option in ["--vc-file", "--branch", "--checkout", "--tag"] {
        let cli = run(&["clink", option, VALUE]);
        assert!(cli.transactions_requested());
        assert!(cli.vc_requested());
        assert!(!cli.persistent_transformations_requested());
    }
    for option in [
        "--always",
        "--once",
        "--never",
        "--triggers",
        "--embed-triggers",
    ] {
        let cli = run(&["clink", option]);
        assert!(cli.persistent_transformations_requested());
        assert!(!cli.transactions_requested());
        assert_eq!(
            cli.trigger_command_count(),
            usize::from(["--always", "--once", "--never"].contains(&option))
        );
    }
    assert!(run(&["clink", "--triggers-file", VALUE]).persistent_transformations_requested());
    assert_eq!(
        run(&["clink", "--always", "--once", "--never"]).trigger_command_count(),
        3
    );
}
