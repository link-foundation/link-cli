//! Regression coverage for repository Dependabot alert 1.

const WASM_CARGO_TOML: &str = include_str!("../Cargo.toml");
const WASM_CARGO_LOCK: &str = include_str!("../Cargo.lock");
const WASM_WRAPPER: &str = include_str!("../src/lib.rs");

#[test]
fn wasm_crate_does_not_use_unmaintained_wee_alloc() {
    for (path, content) in [
        ("rust/wasm/Cargo.toml", WASM_CARGO_TOML),
        ("rust/wasm/Cargo.lock", WASM_CARGO_LOCK),
        ("rust/wasm/src/lib.rs", WASM_WRAPPER),
    ] {
        assert!(
            !content.contains("wee_alloc"),
            "{path} must not reference wee_alloc; GHSA-rc23-xxgq-x27g has no patched version"
        );
    }
}
