//! Independent native page wire and finite persistence contracts; no SQL.
use link_cli::engine::paged::{PageDevice, PagedLog};
use link_cli::engine::replication::{ReplicatedLog, Synchronization};
use link_cli::engine::{
    Constraint, ConstraintValidation, DeltaLog, Engine, EngineOptions, GroupCommitOutcome,
    HistoryPolicy, IsolationLevel, RevisionEncoding, StateStructure,
};
use link_cli::LinksStorage;
use link_cli::{LinkError, TransitionLogStore};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Default, Debug, PartialEq, Eq)]
struct Image {
    metadata: [Option<Vec<u8>>; 2],
    pages: Vec<[u8; 4096]>,
}
#[derive(Default)]
struct State {
    live: Image,
    durable: Image,
    calls: Vec<String>,
    fault: Option<(usize, bool)>,
}
#[derive(Clone, Default)]
struct Device(Rc<RefCell<State>>);
impl Device {
    fn from_image(image: Image) -> Self {
        Self(Rc::new(RefCell::new(State {
            live: image.clone(),
            durable: image,
            ..State::default()
        })))
    }
    fn action(&self, name: String, body: impl FnOnce(&mut State)) -> Result<(), LinkError> {
        let mut s = self.0.borrow_mut();
        s.calls.push(name.clone());
        let fault = s.fault.filter(|(at, _)| *at == s.calls.len());
        if matches!(fault, Some((_, false))) {
            return Err(LinkError::StorageError(format!("injected {name}")));
        }
        body(&mut s);
        if fault.is_some() {
            // A complete unsynced decision may persist before an error reply.
            if name.starts_with("writeMetadata") {
                s.durable.metadata = s.live.metadata.clone();
            }
            return Err(LinkError::StorageError(format!("injected {name}")));
        }
        Ok(())
    }
    fn crash(&self) -> Self {
        Self::from_image(self.0.borrow().durable.clone())
    }
}
impl PageDevice for Device {
    fn supports_durable_flush(&self) -> bool {
        true
    }
    fn new_database_id(&mut self) -> Result<[u8; 16], LinkError> {
        Ok(std::array::from_fn(|i| i as u8))
    }
    fn data_length(&mut self) -> Result<u64, LinkError> {
        Ok(self.0.borrow().live.pages.len() as u64 * 4096)
    }
    fn read_metadata(&mut self, slot: usize) -> Result<Option<Vec<u8>>, LinkError> {
        Ok(self.0.borrow().live.metadata[slot].clone())
    }
    fn read_page(&mut self, index: u64) -> Result<[u8; 4096], LinkError> {
        let mut state = self.0.borrow_mut();
        state.calls.push("readPage".into());
        Ok(state.live.pages[index as usize])
    }
    fn write_page(&mut self, index: u64, bytes: &[u8; 4096]) -> Result<(), LinkError> {
        self.action("writePage".into(), |s| {
            assert_eq!(index as usize, s.live.pages.len());
            s.live.pages.push(*bytes);
        })
    }
    fn sync_data(&mut self) -> Result<(), LinkError> {
        self.action("syncData".into(), |s| {
            s.durable.pages = s.live.pages.clone()
        })
    }
    fn write_metadata(&mut self, slot: usize, bytes: &[u8; 128]) -> Result<(), LinkError> {
        self.action(format!("writeMetadata{slot}"), |s| {
            s.live.metadata[slot] = Some(bytes.to_vec())
        })
    }
    fn sync_metadata(&mut self, slot: usize) -> Result<(), LinkError> {
        self.action(format!("syncMetadata{slot}"), |s| {
            s.durable.metadata[slot] = s.live.metadata[slot].clone()
        })
    }
    fn truncate_data(&mut self, length: u64) -> Result<(), LinkError> {
        self.action("truncateData".into(), |s| {
            s.live.pages.truncate(length as usize / 4096)
        })
    }
}
fn vectors() -> serde_json::Value {
    serde_json::from_str(include_str!("../../docs/protocol/native-pages-v1.json")).unwrap()
}
fn hex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
fn fixture(name: &str) -> (Image, Vec<String>) {
    let value = &vectors()[name];
    (
        Image {
            metadata: std::array::from_fn(|slot| {
                value["metadata"][slot]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(hex)
            }),
            pages: value["pages"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| hex(s.as_str().unwrap()).try_into().unwrap())
                .collect(),
        },
        value["records"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().to_owned())
            .collect(),
    )
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn independent_wire_vectors_and_following_publication() -> Result<(), LinkError> {
    let device = Device::default();
    let mut log = PagedLog::open(device.clone())?;
    assert_eq!(device.0.borrow().live, fixture("empty").0);
    for value in fixture("oneGroup").1 {
        log.append_log_entry(&value)?;
    }
    assert_eq!(log.read_log_entries()?, fixture("oneGroup").1);
    log.flush_log()?;
    assert_eq!(device.0.borrow().live, fixture("oneGroup").0);
    log.append_log_entry("")?;
    log.flush_log()?;
    assert_eq!(device.0.borrow().live, fixture("twoGroups").0);
    for name in ["empty", "oneGroup", "twoGroups"] {
        let (image, records) = fixture(name);
        assert_eq!(
            PagedLog::open(Device::from_image(image))?.read_log_entries()?,
            records
        );
    }
    Ok(())
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn finite_io_failures_fence_and_preserve_acknowledged_prefix() -> Result<(), LinkError> {
    let (image, prior) = fixture("oneGroup");
    let group = vec![
        "雪".repeat(700),
        String::new(),
        "following\0\u{feff}".into(),
    ];
    let probe = Device::from_image(image.clone());
    let mut success = PagedLog::open(probe.clone())?;
    probe.0.borrow_mut().calls.clear();
    for value in &group {
        success.append_log_entry(value)?;
    }
    success.flush_log()?;
    let operations = probe.0.borrow().calls.len();
    assert!((5..=8).contains(&operations));
    for at in 1..=operations {
        for after in [false, true] {
            let device = Device::from_image(image.clone());
            let mut log = PagedLog::open(device.clone())?;
            for value in &group {
                log.append_log_entry(value)?;
            }
            {
                let mut state = device.0.borrow_mut();
                state.calls.clear();
                state.fault = Some((at, after));
            }
            assert!(log
                .flush_log()
                .unwrap_err()
                .to_string()
                .contains("injected"));
            assert!(log.read_log_entries().is_err());
            assert!(log.append_log_entry("rejected").is_err());
            assert!(log.flush_log().is_err());
            let recovered = device.crash();
            let mut reopened = PagedLog::open(recovered.clone())?;
            let survived = reopened.read_log_entries()?;
            assert!(survived == prior || survived == [prior.clone(), group.clone()].concat());
            reopened.append_log_entry("next")?;
            reopened.flush_log()?;
            assert_eq!(
                PagedLog::open(recovered.crash())?.read_log_entries()?,
                [survived, vec!["next".into()]].concat()
            );
        }
    }
    Ok(())
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn committed_page_damage_rejects_without_falling_back() {
    for offset in [0, 8, 24, 32, 40, 44, 48, 56, 64, 4095] {
        let (mut image, _) = fixture("twoGroups");
        image.pages[1][offset] ^= 1;
        let device = Device::from_image(image.clone());
        assert!(PagedLog::open(device.clone()).is_err());
        assert_eq!(device.0.borrow().live, image);
    }
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn torn_unacknowledged_decisions_recover_then_accept_following_writes() -> Result<(), LinkError> {
    for length in [0, 1, 8, 16, 32, 64, 119, 120, 127] {
        let (mut image, _) = fixture("twoGroups");
        image.metadata[0].as_mut().unwrap().truncate(length);
        let device = Device::from_image(image);
        let mut log = PagedLog::open(device.clone())?;
        assert_eq!(log.read_log_entries()?, fixture("oneGroup").1);
        assert_eq!(device.0.borrow().live.pages, fixture("oneGroup").0.pages);
        log.append_log_entry("successor")?;
        log.flush_log()?;
        assert_eq!(
            PagedLog::open(device.crash())?.read_log_entries()?,
            [fixture("oneGroup").1, vec!["successor".into()]].concat()
        );
    }
    Ok(())
}

fn sign_metadata(bytes: &mut [u8]) {
    let sum = independent_hash(bytes[..120].iter().copied());
    bytes[120..128].copy_from_slice(&sum.to_le_bytes());
}
fn independent_hash(bytes: impl Iterator<Item = u8>) -> u64 {
    bytes.fold(14695981039346656037, |sum, byte| {
        (sum ^ u64::from(byte)).wrapping_mul(1099511628211)
    })
}
fn set(image: &mut [u8], offset: usize, value: u64) {
    image[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
fn sign_pages(image: &mut Image) {
    let mut previous = 0;
    for page in &mut image.pages {
        set(page, 48, previous);
        previous = independent_hash(page[..56].iter().chain(page[64..].iter()).copied());
        set(page, 56, previous);
    }
    let slot = if image.pages.len() == 1 { 1 } else { 0 };
    let metadata = image.metadata[slot].as_mut().unwrap();
    set(metadata, 72, previous);
    sign_metadata(metadata);
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn finite_malformed_metadata_never_recreates_or_mutates_files() -> Result<(), LinkError> {
    for (offset, value) in [
        (0, 0),
        (8, 2),
        (12, 1),
        (80, 1),
        (32, 3),
        (40, 2199023255552),
        (48, 253),
        (48, 0),
        (56, 7),
        (56, 0),
        (64, 0),
        (64, 253),
        (32, 1 << 63),
    ] {
        let (mut image, _) = fixture("oneGroup");
        let meta = image.metadata[1].as_mut().unwrap();
        set(meta, offset, value);
        sign_metadata(meta);
        let device = Device::from_image(image.clone());
        assert!(PagedLog::open(device.clone()).is_err());
        assert_eq!(device.0.borrow().live, image);
    }
    let (mut foreign, _) = fixture("oneGroup");
    let meta = foreign.metadata[0].as_mut().unwrap();
    meta[16] ^= 1;
    sign_metadata(meta);
    assert!(PagedLog::open(Device::from_image(foreign)).is_err());
    let (mut identical, records) = fixture("oneGroup");
    identical.metadata[0] = identical.metadata[1].clone();
    assert_eq!(
        PagedLog::open(Device::from_image(identical.clone()))?.read_log_entries()?,
        records
    );
    let meta = identical.metadata[0].as_mut().unwrap();
    set(meta, 48, 5);
    sign_metadata(meta);
    assert!(PagedLog::open(Device::from_image(identical)).is_err());
    let (mut invalid_empty, _) = fixture("empty");
    let meta = invalid_empty.metadata[0].as_mut().unwrap();
    set(meta, 72, 1);
    sign_metadata(meta);
    assert!(PagedLog::open(Device::from_image(invalid_empty)).is_err());
    let (mut missing, _) = fixture("oneGroup");
    missing.metadata = [None, None];
    assert!(PagedLog::open(Device::from_image(missing)).is_err());
    assert!(PagedLog::open(Device::from_image(Image {
        metadata: [Some(vec![0]), None],
        pages: Vec::new()
    }))
    .is_err());
    let (mut truncated, _) = fixture("oneGroup");
    truncated.pages.clear();
    assert!(PagedLog::open(Device::from_image(truncated)).is_err());
    Ok(())
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn checksummed_malformed_doublets_and_counts_are_not_accepted() {
    for (offset, value) in [
        (0, 0),
        (8, 0),
        (24, 1),
        (32, 0),
        (32, 2),
        (40, 0),
        (40, 253),
        (44, 1),
        (4088, 1),
        (64 + 5 * 16, 0),
        (64 + 5 * 16, 4),
        (72 + 5 * 16, 1),
        (64 + 4 * 16, 0),
        (64 + 4 * 16, 0x8800000000000000),
        (64 + 4 * 16, 0x8000000000000000),
        (64 + 4 * 16, 0x8100000000000100),
        (64 + 4 * 16, 0x81000000000000ff),
        (64 + 4 * 16, 0x810000000000000a),
        (72 + 4 * 16, 3),
        (64 + 5 * 16, 0x8000000000000000),
    ] {
        let (mut image, _) = fixture("oneGroup");
        set(&mut image.pages[0], offset, value);
        sign_pages(&mut image);
        let device = Device::from_image(image.clone());
        assert!(PagedLog::open(device.clone()).is_err());
        assert_eq!(device.0.borrow().live, image);
    }
    for (offset, value) in [(48, 5), (56, 2), (64, 5)] {
        let (mut image, _) = fixture("oneGroup");
        let meta = image.metadata[1].as_mut().unwrap();
        set(meta, offset, value);
        sign_metadata(meta);
        assert!(PagedLog::open(Device::from_image(image)).is_err());
    }
    let (mut chain, _) = fixture("oneGroup");
    set(&mut chain.pages[0], 48, 3);
    let page = &mut chain.pages[0];
    let checksum = independent_hash(page[..56].iter().chain(page[64..].iter()).copied());
    set(page, 56, checksum);
    let meta = chain.metadata[1].as_mut().unwrap();
    set(meta, 72, checksum);
    sign_metadata(meta);
    assert!(PagedLog::open(Device::from_image(chain)).is_err());
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn empty_flush_limits_invalid_records_and_live_corruption() -> Result<(), LinkError> {
    use link_cli::engine::paged::MAX_PAGES;
    let device = Device::default();
    for max in [0, MAX_PAGES + 1, u64::MAX] {
        assert!(PagedLog::with_max_pages(device.clone(), max).is_err());
    }
    let mut log = PagedLog::with_max_pages(device.clone(), 1)?;
    let before = device.0.borrow().calls.len();
    log.flush_log()?;
    assert_eq!(device.0.borrow().calls.len(), before);
    assert!(log.supports_durable_flush());
    for invalid in ["\r", "a\nb", "\r\n"] {
        assert!(log.append_log_entry(invalid).is_err());
    }
    assert!(log.read_log_entries()?.is_empty());
    log.append_log_entry("accepted\0😀")?;
    log.flush_log()?;
    assert!(log.append_log_entry("over quota").is_err());
    assert_eq!(log.read_log_entries()?, ["accepted\0😀"]);
    assert!(PagedLog::with_max_pages(Device::from_image(fixture("twoGroups").0), 1).is_err());
    device.0.borrow_mut().live.pages[0][64] ^= 1;
    assert!(log.read_log_entries().is_err());
    assert!(log.append_log_entry("rejected").is_err());
    assert!(log.flush_log().is_err());
    let extracted = log.into_device();
    assert_eq!(extracted.0.borrow().live.pages.len(), 1);
    Ok(())
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn new_group_cost_is_independent_of_committed_history() -> Result<(), LinkError> {
    for history in [0, 1, 32] {
        let device = Device::default();
        let mut log = PagedLog::open(device.clone())?;
        for _ in 0..history {
            log.append_log_entry(&"prior ".repeat(40))?;
            log.flush_log()?;
        }
        let before = device.0.borrow().live.pages.clone();
        device.0.borrow_mut().calls.clear();
        log.append_log_entry("new 雪")?;
        log.flush_log()?;
        let state = device.0.borrow();
        assert_eq!(state.live.pages.len(), history + 1);
        assert_eq!(state.live.pages[..history], before);
        assert_eq!(
            state.calls,
            [
                "writePage".into(),
                "syncData".into(),
                format!("writeMetadata{}", (history + 1) % 2),
                format!("syncMetadata{}", (history + 1) % 2)
            ]
        );
    }
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn repeated_initialization_and_tail_repair_faults_preserve_decisions() -> Result<(), LinkError> {
    for at in 1..=3 {
        for after in [false, true] {
            let device = Device::default();
            device.0.borrow_mut().fault = Some((at, after));
            assert!(PagedLog::open(device.clone()).is_err());
            let recovered = device.crash();
            let mut log = PagedLog::open(recovered.clone())?;
            assert!(log.read_log_entries()?.is_empty());
            log.append_log_entry("following")?;
            log.flush_log()?;
            assert_eq!(
                PagedLog::open(recovered.crash())?.read_log_entries()?,
                ["following"]
            );
        }
    }
    let (mut image, prior) = fixture("oneGroup");
    image.pages.push([0; 4096]);
    let probe = Device::from_image(image.clone());
    PagedLog::open(probe.clone())?;
    let operations = probe.0.borrow().calls.clone();
    for (offset, name) in operations
        .iter()
        .enumerate()
        .filter(|(_, name)| name.as_str() != "readPage")
    {
        for after in [false, true] {
            let device = Device::from_image(image.clone());
            device.0.borrow_mut().fault = Some((offset + 1, after));
            assert!(PagedLog::open(device.clone()).is_err(), "{name}");
            let recovered = device.crash();
            assert_eq!(
                PagedLog::open(recovered.clone())?.read_log_entries()?,
                prior
            );
            assert_eq!(recovered.0.borrow().durable.pages.len(), 1);
        }
    }
    Ok(())
}

fn composed_round<L: TransitionLogStore>(
    log: L,
    options: EngineOptions,
    primary: &Device,
    replica: &Device,
    round: u64,
) -> Result<(), LinkError> {
    let db = Engine::<u64, _>::open(log, options)?;
    let before = db.snapshot()?;
    assert_eq!(before.revision, round * 3);
    assert_eq!(before.rows().len() as u64, round * 3);
    for row in before.rows().values() {
        assert!(matches!(
            row.name.as_deref(),
            Some("\u{feff}雪\0😀" | "second" | "following")
        ));
    }
    primary.0.borrow_mut().calls.clear();
    replica.0.borrow_mut().calls.clear();
    let mut a = db.begin()?;
    let first = a.create_link(0, 0)?;
    a.set_name(first, Some("\u{feff}雪\0😀".into()))?;
    let mut b = db.begin()?;
    let second = b.create_link(first, 0)?;
    b.set_name(second, Some("second".into()))?;
    assert_eq!(
        db.commit_group(vec![a, b])?,
        [
            GroupCommitOutcome::Committed(before.revision + 1),
            GroupCommitOutcome::Committed(before.revision + 2),
        ]
    );
    let syncs = |device: &Device| {
        device
            .0
            .borrow()
            .calls
            .iter()
            .filter(|call| *call == "syncData")
            .count()
    };
    assert_eq!(syncs(primary), 1);
    assert_eq!(syncs(replica), 1);
    assert_eq!(db.snapshot_at(before.revision)?.rows(), before.rows());
    let mut invalid = db.begin()?;
    invalid.create_link(999999, 0)?;
    assert!(invalid.commit().is_err());
    assert_eq!(syncs(primary), 1);
    let mut discarded = db.begin()?;
    discarded.create_link(0, 0)?;
    discarded.rollback();
    let mut following = db.begin()?;
    let third = following.create_link(second, 0)?;
    following.set_name(third, Some("following".into()))?;
    assert_eq!(following.commit()?, before.revision + 3);
    assert_eq!(syncs(primary), 2);
    assert_eq!(syncs(replica), 2);
    assert_eq!(db.snapshot()?.rows().len(), before.rows().len() + 3);
    assert_eq!(
        PagedLog::open(primary.crash())?.read_log_entries()?,
        PagedLog::open(replica.crash())?.read_log_entries()?
    );
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn independent_engine_policies_compose_with_replicas_groups_and_reopen() -> Result<(), LinkError> {
    for state_structure in [StateStructure::Flat, StateStructure::Shared] {
        for revision_encoding in [RevisionEncoding::Snapshot, RevisionEncoding::Changes] {
            for constraint_validation in [ConstraintValidation::Scan, ConstraintValidation::Indexed]
            {
                for delta in [false, true] {
                    let primary = Device::default();
                    let replica = Device::default();
                    for round in 0..2 {
                        let log = ReplicatedLog::new(
                            PagedLog::open(primary.clone())?,
                            vec![PagedLog::open(replica.clone())?],
                            Synchronization::Synchronous {
                                acknowledgements: 2,
                            },
                        )?;
                        let options = EngineOptions {
                            state_structure,
                            revision_encoding,
                            constraint_validation,
                            isolation: IsolationLevel::Snapshot,
                            history: HistoryPolicy::All,
                            constraints: vec![Constraint::References],
                            require_durable_flush: true,
                            ..Default::default()
                        };
                        if delta {
                            composed_round(
                                DeltaLog::new(log, 128)?,
                                options,
                                &primary,
                                &replica,
                                round,
                            )?;
                        } else {
                            composed_round(log, options, &primary, &replica, round)?;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
