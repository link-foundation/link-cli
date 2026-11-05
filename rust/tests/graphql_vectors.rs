//! Runs `docs/protocol/graphql/execute-vectors.json`; the C# tests run the same file.

use link_cli::graphql::{execute, Auth, DeepStore, Json, Settings};
use link_cli::NamedTypesDecorator;

#[test]
fn execute_vectors() {
    let dir = tempfile::tempdir().unwrap();
    let mut links = NamedTypesDecorator::new(dir.path().join("db.links"), false)
        .unwrap()
        .with_indexed_lookups();
    let mut store = DeepStore::open(&mut links, None).unwrap();
    let vectors = Json::parse(include_str!(
        "../../docs/protocol/graphql/execute-vectors.json"
    ))
    .unwrap();
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
            None,
            step.get("variables").unwrap(),
            Auth::default(),
        );
        let passed = match step.get("error").and_then(Json::as_str) {
            Some(code) => answer.write().contains(&format!("\"code\":\"{code}\"")),
            None => answer.get("data").map(Json::write) == step.get("data").map(Json::write),
        };
        if !passed {
            failures.push(format!("{name}: {}", answer.write()));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
