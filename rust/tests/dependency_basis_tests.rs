//! Regression tests for issue-67 Rust basis dependencies.

const CARGO_TOML: &str = include_str!("../Cargo.toml");

fn dependencies_section() -> &'static str {
    CARGO_TOML
        .split("[dependencies]")
        .nth(1)
        .and_then(|rest| rest.split("\n[").next())
        .expect("rust/Cargo.toml should have a [dependencies] section")
}

#[test]
fn rust_manifest_declares_required_basis_crates() {
    let dependencies = dependencies_section();

    for dependency in ["doublets", "links-notation", "lino-arguments"] {
        assert!(
            dependencies.contains(&format!("{dependency} =")),
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

#[test]
fn rust_manifest_uses_lino_arguments_without_direct_clap_dependency() {
    let dependencies = dependencies_section();

    assert!(
        dependencies.contains("lino-arguments ="),
        "rust/Cargo.toml should use lino-arguments as the CLI configuration basis"
    );
    assert!(
        !dependencies.contains("\nclap ="),
        "rust/Cargo.toml should not declare clap directly; lino-arguments owns that integration transitively"
    );
}
