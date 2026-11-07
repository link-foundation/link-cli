//! Rare forms of a query that change only some of what they name.

use link_cli::{import_lino_text, LinkStorage, QueryProcessor};

const TWO: &str = "(1: 1 1)\n(2: 2 2)";

fn run(seed: &str, query: &str) -> (usize, Vec<String>) {
    let mut storage = LinkStorage::in_memory(false);
    import_lino_text(&mut storage, seed).unwrap();
    let changes = QueryProcessor::new(false)
        .process_query(&mut storage, query)
        .unwrap();
    (changes.len(), storage.lino_lines())
}

#[test]
fn a_part_left_open_keeps_its_stored_value() {
    for (query, expected) in [
        ("((1: 1 1)) ((1: 2 *))", "(1: 2 1)"),
        ("((1: 1 1)) ((1: * 2))", "(1: 1 2)"),
        ("((1: 1 1)) ((1: * *))", "(1: 1 1)"),
        ("((1: 1 1)) ((1: *))", "(1: 1 1)"),
        ("((1: 1 1)) ((1: 2))", "(1: 1 1)"),
        ("((1: 1 1)) ((1: ()))", "(1: 1 1)"),
        ("((1: * *)) ((1: 2 *))", "(1: 2 1)"),
        ("(($i: 1 1)) (($i: 2 *))", "(1: 2 1)"),
        ("() ((1: 2 *))", "(1: 2 1)"),
        ("() ((1: * 2))", "(1: 1 2)"),
        ("((2: 2 2)) ((2: 2 2) (1: 2 *))", "(1: 2 1)"),
    ] {
        let (_, rows) = run(TWO, query);
        assert_eq!(rows, [expected, "(2: 2 2)"], "{query}");
    }
}

#[test]
fn a_link_the_substitution_only_repeats_is_reported_unchanged() {
    let (changes, rows) = run(TWO, "((1: 1 1)) ((1: 1 1) (2: 2 2))");
    assert_eq!(changes, 2);
    assert_eq!(rows, ["(1: 1 1)", "(2: 2 2)"]);
}

#[test]
fn a_link_every_solution_plans_is_created_once() {
    let (_, rows) = run("(1: 1 1)\n(2: 1 1)", "(($i: 1 1)) (($i: 1 1) (3: 3 3))");
    assert_eq!(rows.iter().filter(|row| *row == "(3: 3 3)").count(), 1);
}

#[test]
fn a_named_definition_of_an_existing_pair_names_that_pair() {
    let (_, rows) = run(TWO, "((1: 1 1)) ((1: 1 1) (n: 2 2))");
    assert_eq!(rows, ["(1: 1 1)", "(n: n n)"]);
}

#[test]
fn a_link_removed_by_an_earlier_step_of_the_plan_is_put_back() {
    let (_, rows) = run("(1: 1 1)\n(2: 1 1)", "((1: 1 1) (2: 1 1)) ((2: 1 1))");
    assert_eq!(rows, ["(1: 1 1)"]);
}

#[test]
fn every_nested_part_of_a_restriction_is_checked_against_what_it_points_to() {
    const SEED: &str = "(1: 1 1)\n(2: 2 2)\n(3: 2 1)";
    for (restriction, matches) in [
        ("(3: (2: 2 2) (1: 1 1))", true),
        ("(3: (2: 1 2) *)", false),
        ("(3: (2: 2 2) (1: 2 1))", false),
        ("(3: 1 (1: 1))", false),
        // A nested link that names only some parts leaves the others open.
        ("(3: (2: 2) *)", true),
        ("(3: (2:) 1)", true),
        ("(3: (2: *) 1)", true),
        ("(3: (* 2) 1)", true),
        ("(3: ($a: $b) 1)", true),
    ] {
        let query = format!("({restriction}) ((3: 1 2))");
        let (changes, lines) = run(SEED, &query);
        let expected = if matches { "(3: 1 2)" } else { "(3: 2 1)" };
        assert_eq!(changes, usize::from(matches), "{query}");
        assert_eq!(lines[2], expected, "{query}");
    }
}

#[test]
fn a_nested_part_matches_no_link_a_row_points_to_but_that_is_not_there() {
    let (changes, lines) = run("(1: 1 1)\n(3: 9 1)", "((3: (*: 1 1) 1)) ((3: 1 3))");
    assert_eq!(changes, 0);
    assert_eq!(lines, ["(1: 1 1)", "(3: 9 1)"]);
}

#[test]
fn a_query_with_nothing_in_it_changes_nothing() {
    for query in ["", "// only a comment"] {
        assert_eq!(
            run(TWO, query),
            (0, vec!["(1: 1 1)".into(), "(2: 2 2)".into()])
        );
    }
}
