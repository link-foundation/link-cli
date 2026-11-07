//! Rare forms of a `where`, of a table and of a guest.

use link_cli::graphql::plan::{render, Plan};
use link_cli::graphql::tables::{columns, compare, passes, relation};
use link_cli::graphql::{execute, Auth, DeepStore, Json, Settings};
use link_cli::NamedTypesDecorator;
use std::cmp::Ordering;

#[test]
fn an_intersection_renders_its_parts() {
    assert_eq!(
        render(&Plan::Intersect(vec![Plan::All, Plan::Empty])),
        "(intersect all empty)"
    );
}

#[test]
fn every_handler_relation_is_one_link_named_by_a_column_of_the_table() {
    for (name, column) in [
        ("dist", "dist_id"),
        ("execution_provider", "execution_provider_id"),
        ("handler", "handler_id"),
        ("isolation_provider", "isolation_provider_id"),
        ("src", "src_id"),
    ] {
        assert_eq!(relation("handlers", name).unwrap().local, column);
        assert!(columns("handlers").contains(&column), "{column}");
    }
    assert!(relation("handlers", "nothing").is_none());
    assert!(columns("nothing").is_empty());
}

#[test]
fn values_of_different_kinds_never_order() {
    let (yes, no) = (Json::Bool(true), Json::Bool(false));
    assert_eq!(compare(&yes, &no), Some(Ordering::Greater));
    assert_eq!(compare(&yes, &Json::Int(1)), None);
    assert_eq!(
        compare(&Json::Int(2), &Json::Int(1)),
        Some(Ordering::Greater)
    );
}

#[test]
fn not_equal_to_null_is_not_an_answer() {
    assert!(!passes(&Json::Int(1), "_neq", &Json::Null).unwrap());
    assert!(!passes(&Json::Int(1), "_neq", &Json::Int(1)).unwrap());
    assert!(passes(&Json::Int(1), "_neq", &Json::Int(2)).unwrap());
}

#[test]
fn a_guest_of_a_database_with_users_joins_them() {
    let dir = tempfile::tempdir().unwrap();
    let mut links = NamedTypesDecorator::new(dir.path().join("db.links"), false)
        .unwrap()
        .with_indexed_lookups();
    let mut store = DeepStore::open(&mut links, None).unwrap();
    let row = |id: i64, from: i64, to: i64, name: &str| {
        format!(r#"{{"id":{id},"type_id":1,"from_id":{from},"to_id":{to},"string":"{name}"}}"#)
    };
    let package = [
        row(1, 0, 0, "@deep-foundation/core"),
        row(2, 1, 3, "Join"),
        row(3, 0, 0, "the join type"),
        row(4, 1, 5, "User"),
        row(5, 0, 0, "the user type"),
        row(6, 0, 0, "deep"),
        row(7, 6, 8, "users"),
        row(8, 0, 0, "the users"),
    ]
    .join(",");
    store
        .load_package(&Json::parse(&format!("[{package}]")).unwrap())
        .unwrap();
    let settings = Settings::new(b"secret".to_vec(), None);
    let mut run = |query: &str, auth: Auth| {
        execute(&mut store, &settings, query, None, &Json::Null, auth)
            .0
            .write()
    };
    assert_eq!(
        run("query { guest { linkId } }", Auth::default()),
        r#"{"data":{"guest":{"linkId":9}}}"#
    );
    assert_eq!(
        run(
            "query { links(where: { from_id: { _eq: 8 } }) { type_id to_id } }",
            Auth {
                user: None,
                admin: true
            },
        ),
        r#"{"data":{"links":[{"type_id":3,"to_id":9}]}}"#
    );
}

#[test]
fn rows_without_a_value_order_as_postgres_orders_nulls() {
    let dir = tempfile::tempdir().unwrap();
    let mut links = NamedTypesDecorator::new(dir.path().join("db.links"), false)
        .unwrap()
        .with_indexed_lookups();
    let mut store = DeepStore::open(&mut links, None).unwrap();
    let package = r#"[{"id":1,"type_id":1,"from_id":0,"to_id":0},
      {"id":2,"type_id":1,"from_id":0,"to_id":0,"string":"b"},
      {"id":3,"type_id":1,"from_id":0,"to_id":0},
      {"id":4,"type_id":1,"from_id":0,"to_id":0,"string":"a"}]"#;
    store.load_package(&Json::parse(package).unwrap()).unwrap();
    let settings = Settings::new(b"secret".to_vec(), None);
    let admin = Auth {
        user: None,
        admin: true,
    };
    let mut ids = |order: &str| {
        let query = format!("query {{ links(order_by: {order}) {{ id }} }}");
        let reply = execute(
            &mut store,
            &settings,
            &query,
            None,
            &Json::Null,
            admin.clone(),
        );
        reply.0.write()
    };
    for (direction, expected) in [
        ("asc", [4, 2, 1, 3]),
        ("asc_nulls_first", [1, 3, 4, 2]),
        ("desc", [1, 3, 2, 4]),
        ("desc_nulls_last", [2, 4, 1, 3]),
    ] {
        let rows: Vec<_> = expected.map(|id| format!(r#"{{"id":{id}}}"#)).into();
        assert_eq!(
            ids(&format!("{{ string: {{ value: {direction} }} }}")),
            format!(r#"{{"data":{{"links":[{}]}}}}"#, rows.join(",")),
            "{direction}"
        );
    }
    // An order that names a relation and nothing in it keeps the rows in place.
    assert_eq!(
        ids("{ string: { } }"),
        r#"{"data":{"links":[{"id":1},{"id":2},{"id":3},{"id":4}]}}"#
    );
}

#[test]
fn a_mutation_whose_where_cannot_match_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut links = NamedTypesDecorator::new(dir.path().join("db.links"), false)
        .unwrap()
        .with_indexed_lookups();
    let mut store = DeepStore::open(&mut links, None).unwrap();
    let package = r#"[{"id":1,"type_id":1,"from_id":0,"to_id":0}]"#;
    store.load_package(&Json::parse(package).unwrap()).unwrap();
    let settings = Settings::new(b"secret".to_vec(), None);
    let admin = Auth {
        user: None,
        admin: true,
    };
    let mutation = "mutation { delete_links(where: { id: { _in: [] } }) { affected_rows } }";
    let reply = execute(
        &mut store,
        &settings,
        mutation,
        None,
        &Json::Null,
        admin.clone(),
    );
    assert_eq!(
        reply.0.write(),
        r#"{"data":{"delete_links":{"affected_rows":0}}}"#
    );
    let reply = execute(
        &mut store,
        &settings,
        "query { links { id } }",
        None,
        &Json::Null,
        admin,
    );
    assert_eq!(reply.0.write(), r#"{"data":{"links":[{"id":1}]}}"#);
}
