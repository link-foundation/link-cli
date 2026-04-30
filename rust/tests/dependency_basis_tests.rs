//! Regression tests for issue-67 Rust basis dependencies.

const CARGO_TOML: &str = include_str!("../Cargo.toml");

#[test]
fn rust_manifest_declares_required_basis_crates() {
    for dependency in ["doublets", "links-notation", "lino-arguments"] {
        assert!(
            CARGO_TOML.contains(&format!("{dependency} =")),
            "rust/Cargo.toml must declare {dependency} as a direct dependency"
        );
    }

    for source in [
        "http://github.com/linksplatform/doublets-rs",
        "http://github.com/link-foundation/links-notation",
        "http://github.com/link-foundation/lino-arguments",
    ] {
        assert!(
            CARGO_TOML.contains(source),
            "rust/Cargo.toml should document the upstream source {source}"
        );
    }
}
