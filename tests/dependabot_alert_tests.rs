//! Regression coverage for repository Dependabot alert 1.

const ROOT_CARGO_TOML: &str = include_str!("../Cargo.toml");
const ROOT_CARGO_LOCK: &str = include_str!("../Cargo.lock");
const WASM_WRAPPER: &str = include_str!("../src/lib.rs");

#[test]
fn root_wasm_crate_does_not_use_unmaintained_wee_alloc() {
    for (path, content) in [
        ("Cargo.toml", ROOT_CARGO_TOML),
        ("Cargo.lock", ROOT_CARGO_LOCK),
        ("src/lib.rs", WASM_WRAPPER),
    ] {
        assert!(
            !content.contains("wee_alloc"),
            "{path} must not reference wee_alloc; GHSA-rc23-xxgq-x27g has no patched version"
        );
    }
}
