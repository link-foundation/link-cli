//! A whole store survives `--export-binary` and `--import-binary`: links keep
//! their addresses, holes stay holes, names come back, and the archive bytes
//! match the C# port.

use link_cli::cli::{Cli, CliCommand};
use link_cli::protocol::{
    export_store, export_store_file, import_store, import_store_file, LinksPacket, Reference,
    RemoteLinks, ServerOptions, TextLinoProtocol,
};
use link_cli::{Link, NamedTypeLinks, NamedTypesDecorator};
use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use tempfile::{tempdir, TempDir};

mod common;

use common::RunningServer;

/// The archive of [`fill`], byte for byte; the C# `StoreArchiveTests`
/// assert the same bytes.
///
/// - Links packet, 13 bytes: `12` explicit layout, `02` sections; `20 01`
///   one doublet of 1-byte references, `01 01`; `24 01 02` a section after a
///   gap of one address (the hole at 2) with two doublets, `03 03 01 03`.
/// - Names packet, 17 bytes: `13` explicit layout with external references,
///   `02` sections; `21 02` two links of two 2-byte references,
///   `ff ff ff 9f` (1, `a`) and `ff fd ff 17` (3, `é`); `30 01` one link of
///   three 1-byte references, `fc 9f 9e` (4, `a`, `b`).
const ARCHIVE: &str = "12 02 20 01 24 01 02 01 01 03 03 01 03 \
                       13 02 21 02 30 01 ff ff 9f ff fd ff 17 ff fc 9f 9e";

fn store(directory: &TempDir, name: &str) -> NamedTypesDecorator {
    NamedTypesDecorator::new(directory.path().join(name), false).unwrap()
}

/// Links `1: 1 1` named `a`, `3: 3 3` named `é` and `4: 1 3` named `ab`,
/// with a hole at 2.
fn fill(store: &mut impl NamedTypeLinks) {
    for _ in 0..3 {
        let id = store.create(0, 0);
        store.update(id, id, id).unwrap();
    }
    assert_eq!(store.create(1, 3), 4);
    store.delete(2).unwrap();
    store.set_name(1, "a").unwrap();
    store.set_name(3, "é").unwrap();
    store.set_name(4, "ab").unwrap();
}

fn export(store: &mut impl NamedTypeLinks) -> Vec<u8> {
    let mut bytes = Vec::new();
    export_store(store, &mut bytes).unwrap();
    bytes
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn bytes(hex: &str) -> Vec<u8> {
    hex.split_whitespace()
        .map(|byte| u8::from_str_radix(byte, 16).unwrap())
        .collect()
}

fn names(store: &mut impl NamedTypeLinks) -> Vec<(u32, Option<String>)> {
    (1..=4)
        .map(|id| (id, store.get_name(id).unwrap()))
        .collect()
}

fn links(store: &mut impl NamedTypeLinks) -> Vec<Link> {
    let mut links = store.all_links();
    links.sort_by_key(|link| link.index);
    links
}

fn import(bytes: &[u8]) -> anyhow::Result<NamedTypesDecorator> {
    let directory = tempdir().unwrap();
    let mut target = store(&directory, "target.links");
    import_store(&mut target, &mut &bytes[..])?;
    Ok(target)
}

fn import_error(bytes: &[u8]) -> String {
    match import(bytes) {
        Ok(_) => panic!("{} should not import", hex(bytes)),
        Err(error) => error.to_string(),
    }
}

fn packet(external_references: bool, links: &[(u64, Vec<Reference>)]) -> Vec<u8> {
    LinksPacket::pack(external_references, links, true)
        .unwrap()
        .to_bytes()
        .unwrap()
}

fn doublet(address: u64, source: u64, target: u64) -> (u64, Vec<Reference>) {
    (
        address,
        vec![Reference::Internal(source), Reference::Internal(target)],
    )
}

fn no_names() -> Vec<u8> {
    packet(true, &[])
}

#[test]
fn a_store_exports_to_the_golden_archive() {
    let directory = tempdir().unwrap();
    let mut source = store(&directory, "source.links");
    fill(&mut source);

    assert_eq!(hex(&export(&mut source)), hex(&bytes(ARCHIVE)));
}

#[test]
fn an_imported_archive_restores_links_holes_and_names() {
    let directory = tempdir().unwrap();
    let mut source = store(&directory, "source.links");
    fill(&mut source);
    let mut target = store(&directory, "target.links");

    import_store(&mut target, &mut &bytes(ARCHIVE)[..]).unwrap();

    assert_eq!(links(&mut target), links(&mut source));
    assert!(!target.exists(2), "the hole at 2 stays a hole");
    assert_eq!(names(&mut target), names(&mut source));
    assert_eq!(hex(&export(&mut target)), hex(&bytes(ARCHIVE)));
}

#[test]
fn an_empty_store_is_two_empty_packets() {
    let directory = tempdir().unwrap();
    let mut empty = store(&directory, "empty.links");

    assert_eq!(hex(&export(&mut empty)), "10 00 11 00");
    let mut imported = import(&bytes("10 00 11 00")).unwrap();
    assert!(imported.all_links().is_empty());
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
fn an_archive_imports_into_a_store_that_already_has_the_links() {
    let directory = tempdir().unwrap();
    let mut target = store(&directory, "target.links");
    fill(&mut target);

    import_store(&mut target, &mut &bytes(ARCHIVE)[..]).unwrap();

    assert_eq!(hex(&export(&mut target)), hex(&bytes(ARCHIVE)));
}

#[test]
fn a_truncated_archive_is_rejected() {
    assert!(import_error(&[]).contains("ends before its links"));
    assert!(import_error(&bytes("10 00")).contains("ends before its names"));
    assert!(import_error(&bytes("10 00 11")).contains("malformed"));
}

#[test]
fn trailing_bytes_are_rejected() {
    assert!(import_error(&bytes("10 00 11 00 10")).contains("trailing bytes"));
}

#[test]
fn a_link_must_be_a_doublet_of_link_addresses() {
    let triplet = (
        1,
        vec![
            Reference::Internal(1),
            Reference::Internal(1),
            Reference::Internal(1),
        ],
    );
    let external = (1, vec![Reference::External(1), Reference::Internal(1)]);
    for link in [triplet, external] {
        let mut archive = packet(true, &[link]);
        archive.extend(no_names());
        assert!(import_error(&archive).contains("is not a doublet of link addresses"));
    }
}

#[test]
fn addresses_must_fit_a_32_bit_store() {
    let too_large = u64::from(u32::MAX) + 1;
    for link in [doublet(too_large, 1, 1), doublet(1, too_large, 1)] {
        let mut archive = packet(false, &[link]);
        archive.extend(no_names());
        assert!(import_error(&archive).contains("does not fit a 32-bit store"));
    }
}

#[test]
fn names_must_be_an_address_and_code_points() {
    let cases = [
        (vec![Reference::Internal(1)], "only external values"),
        (
            vec![Reference::External(1), Reference::External(0xD800)],
            "invalid code point 55296",
        ),
        (
            vec![Reference::External(1), Reference::External(1 << 40)],
            "invalid code point",
        ),
        (
            vec![Reference::External(1 << 40)],
            "does not fit a 32-bit store",
        ),
    ];
    for (name, message) in cases {
        let mut archive = packet(false, &[doublet(1, 1, 1)]);
        archive.extend(packet(true, &[(1, name)]));
        let error = import_error(&archive);
        assert!(error.contains(message), "{error}");
    }
}

#[test]
fn an_empty_name_is_kept() {
    let mut archive = packet(false, &[doublet(1, 1, 1)]);
    archive.extend(packet(true, &[(1, vec![Reference::External(1)])]));

    let mut imported = import(&archive).unwrap();

    assert_eq!(imported.get_name(1).unwrap().as_deref(), Some(""));
    assert_eq!(hex(&export(&mut imported)), hex(&archive));
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
