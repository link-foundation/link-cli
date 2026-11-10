//! Ordinary sessions and retained exact requests in SQL-free directories.
use link_cli::engine::{
    causal::{CausalPayload, CausalVersion},
    causal_batch::{CausalBatch, CausalBatchEdit},
    causal_projection::CausalConflictPolicy,
    causal_replica::CausalReplicaOptions,
    causal_session::CausalSession,
    cluster::{AtomicJournalStore, JournalVersion},
    lsm::{FileLsmJournalStore, FileLsmOptions, LsmOptions, LsmStoreOptions},
};
use link_cli::protocol::{
    error_message, execute_request_for, format_document, links_operations::parse_changes_for,
    query_document, GenericLinksOperation as Operation, LinoDocument,
};
use link_cli::{LinkError, LinksStorage};
use links_notation::LiNo;
use std::{cell::RefCell, rc::Rc};
fn physical_bytes(
    directory: &str,
) -> std::io::Result<std::collections::BTreeMap<std::ffi::OsString, Vec<u8>>> {
    std::fs::read_dir(directory)?
        .map(|entry| {
            let entry = entry?;
            Ok((entry.file_name(), std::fs::read(entry.path())?))
        })
        .collect()
}
const A: &str = "00000000000000000000000000000001";
fn token(n: u64) -> String {
    format!("{n:032x}")
}
fn row(name: &str) -> Option<CausalPayload> {
    Some(CausalPayload {
        source: 2,
        target: 3,
        name: Some(name.into()),
    })
}
fn request(tx: &mut CausalSession<CutStore>, document: LinoDocument) -> LinoDocument {
    let reply = execute_request_for(
        &mut tx.named_links(),
        &link_cli::GenericQueryProcessor::<u128>::new(false),
        &document,
    );
    assert!(error_message(&reply).is_none(), "{reply:?}");
    reply
}
fn create(tx: &mut CausalSession<CutStore>, id: u128, name: &str) {
    let reply = request(
        tx,
        Operation::<u128>::Create {
            source: 2,
            target: 3,
        }
        .to_document(),
    );
    let changes = parse_changes_for::<u128>(&reply).unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].1.index, id);
    request(tx, Operation::SetName(id, name.into()).to_document());
}
fn change(before: &str, after: Option<&str>) -> LiNo<String> {
    let side = |id: Option<&str>| LiNo::Link {
        id: None,
        values: id
            .map(|id| LiNo::Link {
                id: Some(id.into()),
                values: vec![LiNo::Ref("second".into()), LiNo::Ref("3".into())],
            })
            .into_iter()
            .collect(),
    };
    LiNo::Link {
        id: None,
        values: vec![side(Some(before)), side(after)],
    }
}
fn assert_edits(actual: &[CausalBatchEdit], expected: &[CausalBatchEdit]) {
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected) {
        assert_eq!(actual.address, expected.address);
        assert_eq!(actual.observed, expected.observed);
        assert_eq!(actual.payload, expected.payload);
    }
}
fn prepare(
    tx: &mut CausalSession<CutStore>,
    value: u64,
    generation: u64,
    expected: &[CausalBatchEdit],
) -> Result<(), LinkError> {
    let mut prepared = tx.prepare(&token(value))?;
    assert_eq!(prepared.token, token(value));
    assert_eq!(prepared.generation, generation);
    assert_edits(&prepared.edits, expected);
    if let Some(edit) = prepared.edits.first_mut() {
        edit.address = 99;
    }
    assert_edits(&tx.prepared().unwrap().edits, expected);
    for document in [
        Operation::<u128>::SetName(2, "frozen".into()).to_document(),
        query_document("(($i: $s $t)) (($i: $s $t))"),
    ] {
        let refused = execute_request_for(
            &mut tx.named_links(),
            &link_cli::GenericQueryProcessor::<u128>::new(false),
            &document,
        );
        assert!(error_message(&refused).is_some());
    }
    assert_edits(&tx.prepare(&token(value))?.edits, expected);
    Ok(())
}
#[derive(Clone)]
struct CutStore {
    physical: Rc<RefCell<FileLsmJournalStore>>,
    action: String,
}
impl AtomicJournalStore for CutStore {
    fn supports_durable_flush(&self) -> bool {
        self.physical.borrow().supports_durable_flush()
    }
    fn read(&mut self, keys: &[String]) -> Result<Vec<JournalVersion>, LinkError> {
        self.physical.borrow_mut().read(keys)
    }
    fn compare_exchange(
        &mut self,
        keys: &[String],
        expected: &[JournalVersion],
        updates: &[Option<Vec<String>>],
    ) -> Result<bool, LinkError> {
        if self.action == "cut-before" {
            std::process::exit(73);
        }
        let result = self
            .physical
            .borrow_mut()
            .compare_exchange(keys, expected, updates)?;
        if self.action == "cut-after" && result {
            std::process::exit(73);
        }
        Ok(result)
    }
}
fn batches(store: CutStore) -> Result<CausalBatch<CutStore>, LinkError> {
    CausalBatch::new(
        store,
        "replica",
        "batches",
        A,
        CausalReplicaOptions {
            require_durable_flush: true,
            ..Default::default()
        },
    )
}
fn initial_edits(first: &[CausalVersion]) -> Vec<CausalBatchEdit> {
    first
        .iter()
        .map(|v| CausalBatchEdit {
            address: v.address,
            observed: vec![],
            payload: v.payload.clone(),
        })
        .collect()
}
fn next_edits(first: &[CausalVersion], second: &[CausalVersion]) -> Vec<CausalBatchEdit> {
    second
        .iter()
        .zip(first)
        .map(|(v, prior)| CausalBatchEdit {
            address: v.address,
            observed: vec![prior.clone()],
            payload: v.payload.clone(),
        })
        .collect()
}
fn check(
    owner: &mut CausalBatch<CutStore>,
    phase: usize,
    first: &[CausalVersion],
    second: &[CausalVersion],
    following: &CausalVersion,
) -> Result<(), LinkError> {
    let mut expected = if phase == 0 {
        first.to_vec()
    } else {
        second.to_vec()
    };
    if phase == 2 {
        expected.push(following.clone());
    }
    let mut snapshot = owner.snapshot()?;
    assert_eq!(
        snapshot.generation,
        match phase {
            0 => 1,
            1 => 2,
            // The receipt-only empty batch leaves the owner key unchanged.
            _ => 3,
        }
    );
    assert_eq!(
        snapshot.last_counter,
        match phase {
            0 => 2,
            1 => 4,
            _ => 5,
        }
    );
    assert_eq!(snapshot.versions, expected);
    assert_eq!(owner.write(&token(100), 0, &initial_edits(first))?, first);
    if phase != 0 {
        assert_eq!(
            owner.write(&token(200), 1, &next_edits(first, second))?,
            second
        );
    }
    snapshot.versions[0].clock[0].counter = 99;
    assert_eq!(owner.snapshot()?.versions, expected);
    Ok(())
}
pub fn run(action: &str, directory: &str, mirrors: &str) -> Result<(), LinkError> {
    assert!(matches!(
        action,
        "write"
            | "read"
            | "mutate"
            | "read-advanced"
            | "blocked"
            | "cut-before"
            | "cut-after"
            | "retry-before"
            | "retry-after"
            | "following"
            | "read-following"
    ));
    assert!(matches!(mirrors, "plain" | "mirrored"));
    let mirrored = mirrors == "mirrored";
    let options = FileLsmOptions {
        store: LsmStoreOptions {
            limits: LsmOptions {
                max_keys: 32,
                max_bytes: 65536,
                buffer_keys: 16,
                buffer_bytes: 32768,
                max_runs: 8,
                ..Default::default()
            },
            ..Default::default()
        },
        max_pages: 128,
        mirrored_root: mirrored,
        mirrored_banks: mirrored,
        mirrored_tables: mirrored,
    };
    if action == "blocked" {
        assert!(FileLsmJournalStore::open(directory, options).is_err());
        println!("PASS Rust stale ownership rejected");
        return Ok(());
    }
    if action == "write" {
        assert_eq!(std::fs::read_dir(directory)?.count(), 0);
    }
    let store = CutStore {
        physical: Rc::new(RefCell::new(FileLsmJournalStore::open(directory, options)?)),
        action: action.into(),
    };
    let mut owner = batches(store.clone())?;
    let session = |first, last| {
        CausalSession::new(
            batches(store.clone())?,
            first,
            last,
            CausalConflictPolicy::Reject,
            Default::default(),
        )
    };
    let first = [
        CausalVersion::advance(1, A, 0, &[], row("initial\u{feff}雪😀\0"))?,
        CausalVersion::advance(2, A, 1, &[], row("second"))?,
    ];
    let second = [
        CausalVersion::advance(1, A, 2, &first[..1], None)?,
        CausalVersion::advance(2, A, 3, &first[1..], row("selected\u{feff}雪😀\0"))?,
    ];
    let following = CausalVersion::advance(3, A, 4, &[], row("following"))?;
    if action == "write" {
        let mut tx = session(1, 2)?;
        let bytes = physical_bytes(directory)?;
        let name = first[0].payload.as_ref().unwrap().name.as_deref().unwrap();
        create(&mut tx, 1, name);
        create(&mut tx, 2, "second");
        let matched = request(&mut tx, query_document("(($i: $s $t)) (($i: $s $t))"));
        assert_eq!(
            format_document(&matched),
            format_document(&[change(name, Some(name)), change("second", Some("second"))])
        );
        let literal = links_notation::encode_reference_literal(name);
        assert_eq!(
            format_document(&request(
                &mut tx,
                query_document(&format!("(({literal}: $s $t)) (({literal}: $s $t))"))
            )),
            format_document(&[change(name, Some(name))])
        );
        assert_eq!(tx.rows().len(), 2);
        assert!(owner.snapshot()?.versions.is_empty());
        prepare(&mut tx, 100, 0, &initial_edits(&first))?;
        assert_eq!(physical_bytes(directory)?, bytes);
        assert_eq!(tx.commit(&token(100))?, first);
        drop(tx);
        store.physical.borrow_mut().flush()?;
        store.physical.borrow_mut().compact()?;
    }
    check(
        &mut owner,
        match action {
            "read-advanced" | "retry-after" | "following" => 1,
            "read-following" => 2,
            _ => 0,
        },
        &first,
        &second,
        &following,
    )?;
    if matches!(
        action,
        "mutate" | "cut-before" | "cut-after" | "retry-before" | "retry-after"
    ) {
        if action == "retry-after" {
            // The exact pre-exit request is retained independently of the new snapshot.
            assert_eq!(
                owner.write(&token(200), 1, &next_edits(&first, &second))?,
                second
            );
        } else {
            let mut stale = session(8, 9)?;
            request(
                &mut stale,
                Operation::<u128>::SetName(1, "stale".into()).to_document(),
            );
            let mut tx = session(3, 3)?;
            let private_bytes = physical_bytes(directory)?;
            let observed = request(&mut tx, query_document("(($i: $s $t)) (($i: $s $t))"));
            let original_reply = observed.clone();
            let deleted = request(&mut tx, query_document("((1: $s $t)) ()"));
            assert_eq!(
                format_document(&deleted),
                format_document(&[change("1", None)])
            );
            let name = second[1].payload.as_ref().unwrap().name.as_deref().unwrap();
            request(
                &mut tx,
                Operation::<u128>::SetName(2, name.into()).to_document(),
            );
            assert!(tx.get_link(1).is_none());
            assert_eq!(tx.get_name(2).as_deref(), Some(name));
            assert_eq!(observed, original_reply);
            prepare(&mut tx, 200, 1, &next_edits(&first, &second))?;
            assert_eq!(physical_bytes(directory)?, private_bytes);
            assert_eq!(tx.commit(&token(200))?, second);
            let keys = ["replica".into(), "batches".into()];
            let before = store.physical.borrow_mut().read(&keys)?;
            let bytes = physical_bytes(directory)?;
            assert!(stale.commit(&token(500)).is_err());
            assert_eq!(store.physical.borrow_mut().read(&keys)?, before);
            assert_eq!(physical_bytes(directory)?, bytes);
        }
        check(&mut owner, 1, &first, &second, &following)?;
    }
    if action == "following" {
        let mut tx = session(3, 3)?;
        let bytes = physical_bytes(directory)?;
        create(&mut tx, 3, "following");
        prepare(
            &mut tx,
            300,
            2,
            &[CausalBatchEdit {
                address: 3,
                observed: vec![],
                payload: following.payload.clone(),
            }],
        )?;
        assert_eq!(physical_bytes(directory)?, bytes);
        assert_eq!(
            tx.commit(&token(300))?.as_slice(),
            std::slice::from_ref(&following)
        );
        let mut empty = session(4, 4)?;
        assert!(empty.commit(&token(400))?.is_empty());
        check(&mut owner, 2, &first, &second, &following)?;
    }
    if matches!(action, "mutate" | "retry-before" | "following") {
        store.physical.borrow_mut().flush()?;
        store.physical.borrow_mut().compact()?;
    }
    store.physical.borrow_mut().close()?;
    println!("PASS Rust causal session native LSM {action} {mirrors}");
    Ok(())
}
