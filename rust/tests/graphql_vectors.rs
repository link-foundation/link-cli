//! Runs the vector files of `docs/protocol/graphql`; the C# tests run the same files.

use link_cli::graphql::{execute, Auth, DeepStore, Json, Settings};
use link_cli::NamedTypesDecorator;

fn run(vectors: &str, package: Option<&str>) {
    let dir = tempfile::tempdir().unwrap();
    let mut links = NamedTypesDecorator::new(dir.path().join("db.links"), false)
        .unwrap()
        .with_indexed_lookups();
    let mut store = DeepStore::open(&mut links, None).unwrap();
    let vectors = Json::parse(vectors).unwrap();
    if let Some(package) = package {
        store.load_package(&Json::parse(package).unwrap()).unwrap();
    }
    store.load_package(vectors.get("seed").unwrap()).unwrap();
    let settings = Settings::new(b"secret".to_vec(), None);
    let mut failures = Vec::new();
    for step in vectors.get("steps").unwrap().as_list() {
        let name = step.get("name").and_then(Json::as_str).unwrap();
        let query = step.get("query").and_then(Json::as_str).unwrap();
        let (answer, _) = execute(
            &mut store,
            &settings,
            query,
            step.get("operation").and_then(Json::as_str),
            step.get("variables").unwrap_or(&Json::Null),
            Auth::default(),
        );
        let passed = match step.get("error").and_then(Json::as_str) {
            Some(code) => answer.write().contains(&format!("\"code\":\"{code}\"")),
            None => {
                let expected = step.get("data").expect("a step records data or an error");
                answer.get("data").map(Json::write) == Some(expected.write())
            }
        };
        if !passed {
            failures.push(format!("{name}\t{}", answer.write()));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn execute_vectors() {
    run(
        include_str!("../../docs/protocol/graphql/execute-vectors.json"),
        None,
    );
}

#[test]
fn edge_vectors() {
    run(
        include_str!("../../docs/protocol/graphql/edge-vectors.json"),
        Some(include_str!("../../docs/protocol/graphql/deep-core.json")),
    );
}
