//! A whole store survives `--export-binary` and `--import-binary`: links keep
//! their addresses, holes stay holes, names come back, and the archive bytes
//! match the C# port.

use link_cli::cli::{Cli, CliCommand};
use link_cli::protocol::{
    export_store_file, import_store, import_store_file, RemoteLinks, ServerOptions,
    TextLinoProtocol,
};
use link_cli::{NamedTypeLinks, NamedTypesDecorator};
use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use tempfile::{tempdir, TempDir};

mod common;

use common::RunningServer;

#[path = "common/store_archive.rs"]
mod archive;
use archive::{bytes, export, hex, ARCHIVE};

fn store(directory: &TempDir, name: &str) -> NamedTypesDecorator {
    NamedTypesDecorator::new(directory.path().join(name), false).unwrap()
}

#[test]
fn an_archive_round_trips_through_a_remote_store() {
    let mut server = RunningServer::start(ServerOptions::default(), true);
    let mut remote = RemoteLinks::new(server.client(TextLinoProtocol::new()));

    import_store(&mut remote, &mut &bytes(ARCHIVE)[..]).unwrap();

    assert_eq!(hex(&export(&mut remote)), hex(&bytes(ARCHIVE)));
    assert!(!remote.exists(2));
    server.stop();
}

#[test]
fn the_cli_parses_the_archive_options_and_their_aliases() {
    for (export, import) in [
        ("--export-binary", "--import-binary"),
        ("--binary-output", "--binary-input"),
        ("--binary-out", "--binary-in"),
    ] {
        for arguments in [
            vec![
                export.to_string(),
                "out.bin".into(),
                import.into(),
                "in.bin".into(),
            ],
            vec![format!("{export}=out.bin"), format!("{import}=in.bin")],
        ] {
            let arguments: Vec<&str> = std::iter::once("clink")
                .chain(arguments.iter().map(String::as_str))
                .collect();
            let cli = match Cli::parse_from(&arguments).unwrap() {
                CliCommand::Run(cli) => *cli,
                other => panic!("expected run command, got {other:?}"),
            };
            assert_eq!(cli.binary_output.as_deref(), Some("out.bin"));
            assert_eq!(cli.binary_input.as_deref(), Some("in.bin"));
        }
    }
}

fn clink(database: &Path, arguments: &[&str]) -> Output {
    let output = Command::new(env!("CARGO_BIN_EXE_clink"))
        .arg("--db")
        .arg(database)
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "clink failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn the_cli_copies_a_store_through_an_archive() {
    let directory = tempdir().unwrap();
    let archive = directory.path().join("store.bin");
    let source_lino = directory.path().join("source.lino");
    let target_lino = directory.path().join("target.lino");
    let archive_path = archive.to_str().unwrap();

    clink(
        &directory.path().join("source.links"),
        &[
            "--auto-create-missing-references",
            "() ((child: father mother) (2: 2 1))",
            "--export-binary",
            archive_path,
            "--out",
            source_lino.to_str().unwrap(),
        ],
    );
    clink(
        &directory.path().join("target.links"),
        &[
            "--import-binary",
            archive_path,
            "--out",
            target_lino.to_str().unwrap(),
        ],
    );

    let lino = fs::read_to_string(&target_lino).unwrap();
    assert_eq!(lino, fs::read_to_string(&source_lino).unwrap());
    assert!(lino.contains("(child: father mother)"), "{lino}");
}

#[test]
fn the_cli_imports_the_archive_before_the_lino_file_and_the_query() {
    let directory = tempdir().unwrap();
    let archive = directory.path().join("store.bin");
    let lino = directory.path().join("more.lino");
    fs::write(&archive, bytes(ARCHIVE)).unwrap();
    fs::write(&lino, "(2: 3 4)\n").unwrap();

    let output = clink(
        &directory.path().join("target.links"),
        &[
            "--import-binary",
            archive.to_str().unwrap(),
            "--in",
            lino.to_str().unwrap(),
            "((4: 1 3)) ((4: 4 1))",
            "--after",
        ],
    );

    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "(a: a a)\n(2: é ab)\n(é: é é)\n(ab: ab a)\n"
    );
}

#[test]
fn the_cli_reports_a_missing_archive() {
    let directory = tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_clink"))
        .arg("--db")
        .arg(directory.path().join("target.links"))
        .args(["--import-binary", "missing.bin"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Failed to read the store archive"));
}

#[test]
fn archive_file_failures_name_the_file() {
    let directory = tempdir().unwrap();
    let mut source = store(&directory, "source");
    let missing = directory.path().join("missing").join("store.bin");
    assert_eq!(
        export_store_file(&mut source, &missing)
            .unwrap_err()
            .to_string(),
        format!("Failed to write the store archive: {}", missing.display())
    );
    assert_eq!(
        import_store_file(&mut source, &missing)
            .unwrap_err()
            .to_string(),
        format!("Failed to read the store archive: {}", missing.display())
    );
}
