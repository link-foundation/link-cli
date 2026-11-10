//! Two-bank publication controls use independent finite durable images.
#[path = "common/checkpoint_inspect.rs"]
mod inspection;
use link_cli::engine::paged::{
    CheckpointPagedLog, MirroredPageDevice, PageDevice, RepairPageDevice,
};
use link_cli::{LinkError, ReplaceableTransitionLogStore, TransitionLogStore};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Default)]
struct Image {
    metadata: [Option<Vec<u8>>; 2],
    pages: Vec<[u8; 4096]>,
}
#[derive(Default)]
struct Trace {
    calls: Vec<String>,
    fault: Option<(usize, bool)>,
}
struct State {
    live: Image,
    durable: Image,
}
#[derive(Clone)]
struct Device {
    name: &'static str,
    trace: Rc<RefCell<Trace>>,
    state: Rc<RefCell<State>>,
    flags: u32,
    durable_flush: bool,
    drops: Option<Rc<RefCell<Vec<&'static str>>>>,
}
impl Drop for Device {
    fn drop(&mut self) {
        if let Some(drops) = &self.drops {
            drops.borrow_mut().push(self.name);
        }
    }
}
impl Device {
    fn action<T>(
        &self,
        operation: &str,
        action: impl FnOnce(&mut State) -> T,
    ) -> Result<T, LinkError> {
        let name = format!("{}.{operation}", self.name);
        let fault = {
            let mut trace = self.trace.borrow_mut();
            trace.calls.push(name.clone());
            trace.fault.filter(|(at, _)| *at == trace.calls.len())
        };
        if matches!(fault, Some((_, false))) {
            return Err(LinkError::StorageError(format!("injected {name}")));
        }
        let mut state = self.state.borrow_mut();
        let result = action(&mut state);
        if fault.is_some() {
            if operation.starts_with("writeMetadata") {
                state.durable.metadata = state.live.metadata.clone();
            }
            return Err(LinkError::StorageError(format!("injected {name}")));
        }
        Ok(result)
    }
}
impl PageDevice for Device {
    fn metadata_flags(&self) -> u32 {
        self.flags
    }
    fn supports_durable_flush(&self) -> bool {
        self.durable_flush
    }
    fn new_database_id(&mut self) -> Result<[u8; 16], LinkError> {
        self.action("newDatabaseId", |_| std::array::from_fn(|i| i as u8))
    }
    fn data_length(&mut self) -> Result<u64, LinkError> {
        self.action("dataLength", |s| s.live.pages.len() as u64 * 4096)
    }
    fn read_metadata(&mut self, slot: usize) -> Result<Option<Vec<u8>>, LinkError> {
        self.action(&format!("readMetadata{slot}"), |s| {
            s.live.metadata[slot].clone()
        })
    }
    fn write_metadata(&mut self, slot: usize, bytes: &[u8; 128]) -> Result<(), LinkError> {
        self.action(&format!("writeMetadata{slot}"), |s| {
            s.live.metadata[slot] = Some(bytes.to_vec())
        })
    }
    fn sync_metadata(&mut self, slot: usize) -> Result<(), LinkError> {
        self.action(&format!("syncMetadata{slot}"), |s| {
            s.durable.metadata[slot] = s.live.metadata[slot].clone()
        })
    }
    fn read_page(&mut self, index: u64) -> Result<[u8; 4096], LinkError> {
        self.action("readPage", |s| s.live.pages[index as usize])
    }
    fn write_page(&mut self, index: u64, bytes: &[u8; 4096]) -> Result<(), LinkError> {
        self.action("writePage", |s| {
            assert_eq!(index as usize, s.live.pages.len());
            s.live.pages.push(*bytes);
        })
    }
    fn sync_data(&mut self) -> Result<(), LinkError> {
        self.action("syncData", |s| s.durable.pages = s.live.pages.clone())
    }
    fn truncate_data(&mut self, length: u64) -> Result<(), LinkError> {
        self.action("truncateData", |s| {
            s.live.pages.truncate(length as usize / 4096)
        })
    }
    fn complete_recovery(&mut self) -> Result<(), LinkError> {
        self.action("completeRecovery", |_| ())
    }
}
impl RepairPageDevice for Device {
    fn repair_page(&mut self, index: u64, bytes: &[u8; 4096]) -> Result<(), LinkError> {
        self.action("repairPage", |s| s.live.pages[index as usize] = *bytes)
    }
}
type Log = CheckpointPagedLog<Device, Device>;
fn devices<const N: usize>(images: [Image; N]) -> ([Device; N], Rc<RefCell<Trace>>) {
    let trace = Rc::new(RefCell::new(Trace::default()));
    let devices = std::array::from_fn(|i| Device {
        name: ["root", "bank0", "bank1", "mirror0", "mirror1", "rootMirror"][i],
        trace: trace.clone(),
        state: Rc::new(RefCell::new(State {
            live: images[i].clone(),
            durable: images[i].clone(),
        })),
        flags: 0,
        durable_flush: true,
        drops: None,
    });
    (devices, trace)
}
fn setup(images: [Image; 3]) -> (Log, [Device; 3], Rc<RefCell<Trace>>) {
    let (devices, trace) = devices(images);
    let log = CheckpointPagedLog::with_max_pages(
        devices[0].clone(),
        [devices[1].clone(), devices[2].clone()],
        4,
    )
    .unwrap();
    (log, devices, trace)
}
fn durable<const N: usize>(devices: &[Device; N]) -> [Image; N] {
    std::array::from_fn(|i| devices[i].state.borrow().durable.clone())
}
fn vectors() -> serde_json::Value {
    serde_json::from_str(include_str!("../../docs/protocol/checkpoint-pages-v1.json")).unwrap()
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn bytes(value: &serde_json::Value) -> Vec<u8> {
    let value = value.as_str().unwrap().as_bytes();
    value
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u8::from_str_radix(std::str::from_utf8(c).unwrap(), 16).unwrap())
        .collect()
}
fn open(devices: &[Device; 3]) -> Result<Log, LinkError> {
    CheckpointPagedLog::with_max_pages(
        devices[0].clone(),
        [devices[1].clone(), devices[2].clone()],
        4,
    )
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn malformed_roots_selected_bank_damage_and_unrooted_data_never_trigger_repair_writes() {
    let vectors = vectors();
    let (mut log, source, _) = setup(Default::default());
    log.append_log_entry("old").unwrap();
    log.flush_log().unwrap();
    log.replace_log_entries(&["new".into()]).unwrap();
    let images = durable(&source);
    let mut invalid = Vec::new();
    for root in vectors["invalid"].as_array().unwrap() {
        let mut image = images.clone();
        image[0].metadata[0] = Some(bytes(root));
        invalid.push(image);
    }
    for decision in [&vectors["decisions"]["3"], &vectors["foreignDecision"]] {
        let mut image = images.clone();
        image[0].metadata[1] = Some(bytes(decision));
        invalid.push(image);
    }
    let mut wrong_slot = images.clone();
    wrong_slot[0].metadata.swap(0, 1);
    invalid.push(wrong_slot);
    let mut root_data = images.clone();
    root_data[0].pages.push([0; 4096]);
    invalid.push(root_data);
    let mut missing = images.clone();
    missing[2].metadata = [None, None];
    invalid.push(missing);
    let mut stale = images.clone();
    stale[2].metadata = images[1].metadata.clone();
    invalid.push(stale);
    let mut damaged = images.clone();
    damaged[2].pages[0][56] ^= 1;
    invalid.push(damaged);
    let mut unrooted = images.clone();
    unrooted[0].metadata = [None, None];
    invalid.push(unrooted);
    let mut unrooted_metadata = images.clone();
    unrooted_metadata[0].metadata = [None, None];
    unrooted_metadata[1].pages.clear();
    unrooted_metadata[2].pages.clear();
    invalid.push(unrooted_metadata);
    for image in invalid {
        let (devices, trace) = devices(image.clone());
        assert!(open(&devices).is_err());
        assert!(trace
            .borrow()
            .calls
            .iter()
            .all(|name| name.contains("read") || name.ends_with("dataLength")));
        for i in 0..3 {
            assert_eq!(devices[i].state.borrow().live.metadata, image[i].metadata);
            assert_eq!(devices[i].state.borrow().live.pages, image[i].pages);
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn every_torn_root_length_recovers_the_prior_acknowledgement() {
    let (mut log, source, _) = setup(Default::default());
    log.append_log_entry("old").unwrap();
    log.flush_log().unwrap();
    log.replace_log_entries(&["new".into()]).unwrap();
    let complete = durable(&source);
    for length in 0..128 {
        let mut image = complete.clone();
        image[0].metadata[1].as_mut().unwrap().truncate(length);
        let (mut reopened, _, _) = setup(image);
        assert_eq!(reopened.epoch(), 0);
        assert_eq!(reopened.read_log_entries().unwrap(), ["old"]);
    }
    let mut image = complete;
    image[0].metadata[1].as_mut().unwrap()[120] ^= 1;
    assert_eq!(setup(image).0.read_log_entries().unwrap(), ["old"]);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn exact_u64_epochs_exhaustion_device_modes_and_extraction_are_bounded() {
    let vectors = vectors();
    for epoch in [9007199254740993_u64, u64::MAX] {
        let mut image: [Image; 3] = Default::default();
        image[0].metadata[1] = Some(bytes(&vectors["decisions"][epoch.to_string()]));
        image[2].metadata = [Some(bytes(&vectors["emptyBanks"][epoch.to_string()])), None];
        let (mut log, source, trace) = setup(image);
        assert_eq!(log.epoch(), epoch);
        assert!(!log.replace_log_entries(&[]).unwrap());
        trace.borrow_mut().calls.clear();
        if epoch == u64::MAX {
            assert!(log
                .replace_log_entries(&["new".into()])
                .unwrap_err()
                .to_string()
                .contains("exhausted"));
            assert!(trace
                .borrow()
                .calls
                .iter()
                .all(|name| name.ends_with("completeRecovery")));
            log.append_log_entry("following").unwrap();
            log.flush_log().unwrap();
        } else {
            assert!(log.replace_log_entries(&["new".into()]).unwrap());
            assert_eq!(log.epoch(), epoch + 1);
        }
        let (root, banks) = log.into_devices();
        assert!(Rc::ptr_eq(&root.state, &source[0].state));
        for i in 0..2 {
            assert!(Rc::ptr_eq(&banks[i].state, &source[i + 1].state));
        }
        let mut reopened = CheckpointPagedLog::open(root, banks).unwrap();
        assert_eq!(
            reopened.read_log_entries().unwrap(),
            if epoch == u64::MAX {
                vec!["following"]
            } else {
                vec!["new"]
            }
        );
    }
    for flags in [[1, 0, 0], [0, 2, 2], [0, 0, 1]] {
        let (mut devices, trace) = devices::<3>(Default::default());
        for i in 0..3 {
            devices[i].flags = flags[i];
        }
        assert!(open(&devices).is_err());
        assert!(trace.borrow().calls.is_empty());
    }
    for max in [0, u64::MAX] {
        let (devices, trace) = devices::<3>(Default::default());
        assert!(CheckpointPagedLog::with_max_pages(
            devices[0].clone(),
            [devices[1].clone(), devices[2].clone()],
            max
        )
        .is_err());
        assert!(trace.borrow().calls.is_empty());
    }
    for disabled in 0..3 {
        let (mut devices, _) = devices::<3>(Default::default());
        devices[disabled].durable_flush = false;
        assert!(!open(&devices).unwrap().supports_durable_flush());
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn every_initialization_failure_reopens_empty_without_committing_records() {
    let (_, _, trace) = setup(Default::default());
    let operations = trace.borrow().calls.len();
    assert!((20..=40).contains(&operations));
    for at in 1..=operations {
        for after in [false, true] {
            let (devices, trace) = devices::<3>(Default::default());
            trace.borrow_mut().fault = Some((at, after));
            assert!(open(&devices).is_err());
            let (mut reopened, _, _) = setup(durable(&devices));
            assert!(reopened.read_log_entries().unwrap().is_empty());
            reopened.append_log_entry("after recovery").unwrap();
            reopened.flush_log().unwrap();
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn mirrored_banks_repair_only_the_selected_epoch_then_accept_repeated_swaps() {
    let (source, _) = devices::<5>(Default::default());
    let mirrors = || {
        [
            MirroredPageDevice::new(source[1].clone(), source[2].clone()),
            MirroredPageDevice::new(source[3].clone(), source[4].clone()),
        ]
    };
    let mut log = CheckpointPagedLog::open(source[0].clone(), mirrors()).unwrap();
    log.append_log_entry("old").unwrap();
    log.flush_log().unwrap();
    assert!(log.replace_log_entries(&["new".into()]).unwrap());
    assert_eq!(
        hex(source[0].state.borrow().live.metadata[1].as_ref().unwrap()),
        vectors()["mirroredDecision"]
    );
    let mut images = durable(&source);
    images[3].pages[0][56] ^= 1;
    images[3].metadata[0] = Some(vec![0; 17]);
    let (source, _) = devices(images);
    let mut log = CheckpointPagedLog::open(
        source[0].clone(),
        [
            MirroredPageDevice::new(source[1].clone(), source[2].clone()),
            MirroredPageDevice::new(source[3].clone(), source[4].clone()),
        ],
    )
    .unwrap();
    assert_eq!(log.read_log_entries().unwrap(), ["new"]);
    assert_eq!(
        source[3].state.borrow().durable.pages,
        source[4].state.borrow().durable.pages
    );
    assert_eq!(
        source[3].state.borrow().durable.metadata,
        source[4].state.borrow().durable.metadata
    );
    assert!(log.replace_log_entries(&["third".into()]).unwrap());
    assert!(log.replace_log_entries(&["fourth".into()]).unwrap());
    assert_eq!(log.read_log_entries().unwrap(), ["fourth"]);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn mirrored_replacement_failures_reopen_old_or_new_and_reuse_the_partial_inactive_bank() {
    fn open(source: &[Device; 5]) -> CheckpointPagedLog<Device, MirroredPageDevice<Device>> {
        CheckpointPagedLog::with_max_pages(
            source[0].clone(),
            [
                MirroredPageDevice::new(source[1].clone(), source[2].clone()),
                MirroredPageDevice::new(source[3].clone(), source[4].clone()),
            ],
            4,
        )
        .unwrap()
    }
    let (source, _) = devices::<5>(Default::default());
    let mut log = open(&source);
    log.append_log_entry("old").unwrap();
    log.flush_log().unwrap();
    let initial = durable(&source);
    let replacement = vec!["雪".repeat(700), "\u{feff}\0😀".into()];
    let (source, trace) = devices(initial.clone());
    let mut log = open(&source);
    trace.borrow_mut().calls.clear();
    log.replace_log_entries(&replacement).unwrap();
    let operations = trace.borrow().calls.len();
    assert!((30..=90).contains(&operations));
    for at in 1..=operations {
        for after in [false, true] {
            let (source, trace) = devices(initial.clone());
            let mut log = open(&source);
            trace.borrow_mut().calls.clear();
            trace.borrow_mut().fault = Some((at, after));
            if log.replace_log_entries(&replacement).is_err() {
                let count = trace.borrow().calls.len();
                assert!(log.read_log_entries().is_err());
                assert!(log.replace_log_entries(&["rejected".into()]).is_err());
                assert_eq!(trace.borrow().calls.len(), count);
            }
            let (source, _) = devices(durable(&source));
            let mut reopened = open(&source);
            let found = reopened.read_log_entries().unwrap();
            assert!(found == ["old"] || found == replacement);
            reopened
                .replace_log_entries(&["after recovery".into()])
                .unwrap();
            let (source, _) = devices(durable(&source));
            assert_eq!(
                open(&source).read_log_entries().unwrap(),
                ["after recovery"]
            );
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn ordinary_read_and_flush_io_failures_fence_every_operation() {
    for flush in [false, true] {
        let (mut log, _, trace) = setup(Default::default());
        log.append_log_entry("old").unwrap();
        log.flush_log().unwrap();
        assert!(log.append_log_entry("a\nb").is_err());
        if flush {
            log.append_log_entry("pending").unwrap();
        }
        trace.borrow_mut().calls.clear();
        trace.borrow_mut().fault = Some((1, false));
        assert!(if flush {
            log.flush_log().map(|_| Vec::new())
        } else {
            log.read_log_entries()
        }
        .is_err());
        let count = trace.borrow().calls.len();
        assert!(log.read_log_entries().is_err());
        assert!(log.append_log_entry("rejected").is_err());
        assert!(log.flush_log().is_err());
        assert_eq!(trace.borrow().calls.len(), count);
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn independent_root_vectors_repeated_swaps_and_following_groups() {
    let (mut log, devices, _) = setup(Default::default());
    assert_eq!(
        hex(devices[0].state.borrow().live.metadata[0].as_ref().unwrap()),
        vectors()["decisions"]["0"]
    );
    log.append_log_entry("old").unwrap();
    log.flush_log().unwrap();
    for epoch in 1..=2 {
        let replacement = vec![
            "雪".repeat(700),
            format!("\u{feff}\0😀 {epoch}"),
            String::new(),
        ];
        assert!(log.replace_log_entries(&replacement).unwrap());
        assert_eq!(log.read_log_entries().unwrap(), replacement);
        assert_eq!(
            hex(devices[0].state.borrow().live.metadata[epoch % 2]
                .as_ref()
                .unwrap()),
            vectors()["decisions"][epoch.to_string()]
        );
        let (mut reopened, _, _) = setup(durable(&devices));
        assert_eq!(reopened.read_log_entries().unwrap(), replacement);
        log.append_log_entry("following").unwrap();
        log.flush_log().unwrap();
        assert_eq!(
            log.read_log_entries().unwrap(),
            [replacement, vec!["following".into()]].concat()
        );
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn every_replacement_io_failure_fences_and_reopens_complete_old_or_new_records() {
    let (mut start, devices, _) = setup(Default::default());
    start.append_log_entry("acknowledged").unwrap();
    start.flush_log().unwrap();
    let images = durable(&devices);
    let replacement = vec!["雪".repeat(700), "\u{feff}\0😀".into(), String::new()];
    let (mut success, _, trace) = setup(images.clone());
    trace.borrow_mut().calls.clear();
    success.replace_log_entries(&replacement).unwrap();
    let operations = trace.borrow().calls.len();
    assert!(
        (12..=40).contains(&operations),
        "{:?}",
        trace.borrow().calls
    );
    for at in 1..=operations {
        for after in [false, true] {
            let (mut log, devices, trace) = setup(images.clone());
            trace.borrow_mut().calls.clear();
            trace.borrow_mut().fault = Some((at, after));
            assert!(log
                .replace_log_entries(&replacement)
                .unwrap_err()
                .to_string()
                .contains("injected"));
            let count = trace.borrow().calls.len();
            assert!(log.read_log_entries().is_err());
            assert!(log.append_log_entry("rejected").is_err());
            assert!(log.flush_log().is_err());
            assert!(log.replace_log_entries(&["rejected".into()]).is_err());
            assert_eq!(trace.borrow().calls.len(), count);
            let (mut reopened, devices, _) = setup(durable(&devices));
            let found = reopened.read_log_entries().unwrap();
            assert!(found == ["acknowledged"] || found == replacement);
            reopened.append_log_entry("next").unwrap();
            reopened.flush_log().unwrap();
            let (mut again, _, _) = setup(durable(&devices));
            assert_eq!(
                again.read_log_entries().unwrap(),
                [found, vec!["next".into()]].concat()
            );
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn invalid_pending_and_identical_replacements_preserve_a_usable_log() {
    let (mut log, _, trace) = setup(Default::default());
    log.append_log_entry("old").unwrap();
    log.flush_log().unwrap();
    trace.borrow_mut().calls.clear();
    assert!(!log.replace_log_entries(&["old".into()]).unwrap());
    assert!(trace
        .borrow()
        .calls
        .iter()
        .all(|call| call.ends_with(".readPage") || call.ends_with(".completeRecovery")));
    for invalid in ["a\nb".into(), "\r".into(), "x".repeat(10000)] {
        assert!(log.replace_log_entries(&[invalid]).is_err());
    }
    assert_eq!(log.read_log_entries().unwrap(), ["old"]);
    log.append_log_entry("pending").unwrap();
    assert!(log
        .replace_log_entries(&["new".into()])
        .unwrap_err()
        .to_string()
        .contains("pending"));
    log.flush_log().unwrap();
    assert_eq!(log.read_log_entries().unwrap(), ["old", "pending"]);
    assert!(log.replace_log_entries(&[]).unwrap());
    assert!(log.read_log_entries().unwrap().is_empty());
    assert!(log.supports_durable_flush());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn owning_device_destruction_releases_root_after_both_banks() {
    for replace in [false, true] {
        let (mut source, _) = devices::<3>(Default::default());
        let drops = Rc::new(RefCell::new(Vec::new()));
        for device in &mut source {
            device.drops = Some(drops.clone());
        }
        let [root, first, second] = source;
        let mut log = CheckpointPagedLog::open(root, [first, second]).unwrap();
        if replace {
            log.replace_log_entries(&["new".into()]).unwrap();
        }
        assert!(drops.borrow().is_empty());
        drop(log);
        let found = drops.borrow();
        assert_eq!(found.len(), 3);
        assert_eq!(
            found[2], "root",
            "root ownership must outlive both banks: {found:?}"
        );
        assert!(found[..2].contains(&"bank0") && found[..2].contains(&"bank1"));
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn failed_open_releases_banks_before_root_at_every_device_operation() {
    let (mut initialized, source, _) = setup(Default::default());
    initialized
        .replace_log_entries(&["committed".into()])
        .unwrap();
    for image in [Default::default(), durable(&source)] {
        let (control, baseline) = devices(image.clone());
        let [root, first, second] = control;
        drop(CheckpointPagedLog::open(root, [first, second]).unwrap());
        let count = baseline.borrow().calls.len();
        assert!((10..=50).contains(&count));
        for at in 1..=count {
            for after in [false, true] {
                let (mut source, trace) = devices(image.clone());
                let drops = Rc::new(RefCell::new(Vec::new()));
                for device in &mut source {
                    device.drops = Some(drops.clone());
                }
                trace.borrow_mut().fault = Some((at, after));
                let [root, first, second] = source;
                assert!(CheckpointPagedLog::open(root, [first, second]).is_err());
                let found = drops.borrow();
                assert_eq!(found.len(), 3);
                assert_eq!(found[2], "root", "open fault {at}/{after}: {found:?}");
                assert!(found[..2].contains(&"bank0") && found[..2].contains(&"bank1"));
            }
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn engine_compaction_composes_with_both_bank_modes_delta_state_history_and_encoding() {
    use link_cli::engine::{
        DeltaLog, Engine, EngineOptions, HistoryPolicy, RevisionEncoding, StateStructure,
    };
    use link_cli::LinksStorage;
    fn exercise<L: ReplaceableTransitionLogStore>(log: L, options: EngineOptions) {
        let engine = Engine::<u32, _>::open(log, options).unwrap();
        assert!(engine.compact_journal().unwrap());
        let mut pin = None;
        for revision in 1..=6 {
            let mut tx = engine.begin().unwrap();
            if revision == 1 {
                tx.create_link(0, 0).unwrap();
                tx.ensure_link_created(7).unwrap();
                tx.delete_link(7).unwrap();
            }
            tx.set_name(1, Some(format!("雪 {} {revision}", "x".repeat(256))))
                .unwrap();
            tx.commit().unwrap();
            if revision == 1 {
                pin = Some(engine.snapshot().unwrap());
            }
        }
        let mut pending = engine.begin().unwrap();
        assert_eq!(pending.create_link(0, 0).unwrap(), 8);
        let expected = engine.export_checkpoint().unwrap();
        let changed = engine.with_log(|log| log.read_log_entries()).unwrap() != expected;
        assert_eq!(engine.compact_journal().unwrap(), changed);
        assert!(!engine.compact_journal().unwrap());
        assert_eq!(
            engine.with_log(|log| log.read_log_entries()).unwrap(),
            expected
        );
        assert!(engine.snapshot().unwrap().get(8).is_none());
        pending.commit().unwrap();
        drop(pending);
        assert!(pin
            .unwrap()
            .get(1)
            .unwrap()
            .name
            .as_ref()
            .unwrap()
            .ends_with(" 1"));
        engine.compact_journal().unwrap();
        assert!(!engine.compact_journal().unwrap());
    }
    fn reopened<L: ReplaceableTransitionLogStore>(log: L, options: EngineOptions) {
        let engine = Engine::<u32, _>::open(log, options).unwrap();
        assert_eq!(engine.snapshot().unwrap().revision, 7);
        assert!(engine.snapshot().unwrap().get(8).is_some());
        assert!(!engine.compact_journal().unwrap());
        let mut following = engine.begin().unwrap();
        assert_eq!(following.create_link(0, 0).unwrap(), 9);
        following.commit().unwrap();
        engine.compact_journal().unwrap();
        assert!(!engine.compact_journal().unwrap());
    }
    for state_structure in [
        StateStructure::Flat,
        StateStructure::Shared,
        StateStructure::SortedRuns,
    ] {
        for revision_encoding in [RevisionEncoding::Snapshot, RevisionEncoding::Changes] {
            for history in [
                HistoryPolicy::None,
                HistoryPolicy::Last(2),
                HistoryPolicy::All,
            ] {
                for delta in [false, true] {
                    let options = EngineOptions {
                        state_structure,
                        revision_encoding,
                        history,
                        require_durable_flush: true,
                        ..Default::default()
                    };
                    let (source, _) = devices::<3>(Default::default());
                    let log = CheckpointPagedLog::with_max_pages(
                        source[0].clone(),
                        [source[1].clone(), source[2].clone()],
                        64,
                    )
                    .unwrap();
                    if delta {
                        exercise(DeltaLog::new(log, 128).unwrap(), options.clone());
                    } else {
                        exercise(log, options.clone());
                    }
                    let (source, _) = devices(durable(&source));
                    let log = CheckpointPagedLog::with_max_pages(
                        source[0].clone(),
                        [source[1].clone(), source[2].clone()],
                        64,
                    )
                    .unwrap();
                    if delta {
                        reopened(DeltaLog::new(log, 128).unwrap(), options.clone());
                    } else {
                        reopened(log, options.clone());
                    }
                    let (source, _) = devices::<5>(Default::default());
                    let mirrored = |source: &[Device; 5]| {
                        CheckpointPagedLog::with_max_pages(
                            source[0].clone(),
                            [
                                MirroredPageDevice::new(source[1].clone(), source[2].clone()),
                                MirroredPageDevice::new(source[3].clone(), source[4].clone()),
                            ],
                            64,
                        )
                        .unwrap()
                    };
                    if delta {
                        exercise(
                            DeltaLog::new(mirrored(&source), 128).unwrap(),
                            options.clone(),
                        );
                    } else {
                        exercise(mirrored(&source), options.clone());
                    }
                    let (source, _) = devices(durable(&source));
                    if delta {
                        reopened(DeltaLog::new(mirrored(&source), 128).unwrap(), options);
                    } else {
                        reopened(mirrored(&source), options);
                    }
                }
            }
        }
    }
}

#[path = "support/checkpoint_root.rs"]
mod root_mirror;
