use super::*;
use link_cli::engine::paged::MirroredCheckpointRoot;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn mirrored_root_recovers_acknowledged_epoch_after_one_copy_damage() {
    let (devices, _) = devices::<4>(Default::default());
    let root = MirroredCheckpointRoot::new(devices[0].clone(), devices[3].clone(), 0).unwrap();
    let mut log =
        CheckpointPagedLog::with_max_pages(root, [devices[1].clone(), devices[2].clone()], 4)
            .unwrap();
    log.append_log_entry("old").unwrap();
    log.flush_log().unwrap();
    log.replace_log_entries(&["acknowledged".into()]).unwrap();
    let mut images = durable(&devices);
    images[0].metadata[1].as_mut().unwrap()[120] ^= 1;
    let (reopened, _) = super::devices(images);
    let root = MirroredCheckpointRoot::new(reopened[0].clone(), reopened[3].clone(), 0).unwrap();
    let mut log =
        CheckpointPagedLog::with_max_pages(root, [reopened[1].clone(), reopened[2].clone()], 4)
            .unwrap();
    assert_eq!(log.epoch(), 1);
    assert_eq!(log.read_log_entries().unwrap(), ["acknowledged"]);
    assert_eq!(
        durable(&reopened)[0].metadata,
        durable(&reopened)[3].metadata
    );
}

// One test adapter keeps the same independent images for both bank modes.
enum Bank {
    Raw(Device),
    Mirror(MirroredPageDevice<Device>),
}
impl Bank {
    fn device(&mut self) -> &mut dyn PageDevice {
        match self {
            Self::Raw(d) => d,
            Self::Mirror(d) => d,
        }
    }
}
impl PageDevice for Bank {
    fn metadata_flags(&self) -> u32 {
        u32::from(matches!(self, Self::Mirror(_)))
    }
    fn supports_durable_flush(&self) -> bool {
        match self {
            Self::Raw(d) => d.supports_durable_flush(),
            Self::Mirror(d) => d.supports_durable_flush(),
        }
    }
    fn new_database_id(&mut self) -> Result<[u8; 16], LinkError> {
        self.device().new_database_id()
    }
    fn data_length(&mut self) -> Result<u64, LinkError> {
        self.device().data_length()
    }
    fn read_metadata(&mut self, slot: usize) -> Result<Option<Vec<u8>>, LinkError> {
        self.device().read_metadata(slot)
    }
    fn write_metadata(&mut self, slot: usize, bytes: &[u8; 128]) -> Result<(), LinkError> {
        self.device().write_metadata(slot, bytes)
    }
    fn sync_metadata(&mut self, slot: usize) -> Result<(), LinkError> {
        self.device().sync_metadata(slot)
    }
    fn read_page(&mut self, index: u64) -> Result<[u8; 4096], LinkError> {
        self.device().read_page(index)
    }
    fn read_checked_page(&mut self, index: u64, expected: u64) -> Result<[u8; 4096], LinkError> {
        self.device().read_checked_page(index, expected)
    }
    fn write_page(&mut self, index: u64, bytes: &[u8; 4096]) -> Result<(), LinkError> {
        self.device().write_page(index, bytes)
    }
    fn sync_data(&mut self) -> Result<(), LinkError> {
        self.device().sync_data()
    }
    fn truncate_data(&mut self, length: u64) -> Result<(), LinkError> {
        self.device().truncate_data(length)
    }
    fn complete_recovery(&mut self) -> Result<(), LinkError> {
        self.device().complete_recovery()
    }
}
type RootLog = CheckpointPagedLog<MirroredCheckpointRoot<Device>, Bank>;
fn banks(devices: &[Device; 6], flags: u32) -> [Bank; 2] {
    std::array::from_fn(|i| {
        if flags == 1 {
            Bank::Mirror(MirroredPageDevice::new(
                devices[i + 1].clone(),
                devices[i + 3].clone(),
            ))
        } else {
            Bank::Raw(devices[i + 1].clone())
        }
    })
}
fn root_open(devices: &[Device; 6], flags: u32) -> Result<RootLog, LinkError> {
    CheckpointPagedLog::with_max_pages(
        MirroredCheckpointRoot::new(devices[0].clone(), devices[5].clone(), flags)?,
        banks(devices, flags),
        4,
    )
}
fn root_setup(images: [Image; 6], flags: u32) -> (RootLog, [Device; 6], Rc<RefCell<Trace>>) {
    let (devices, trace) = devices(images);
    (root_open(&devices, flags).unwrap(), devices, trace)
}
fn root_vectors() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../docs/protocol/checkpoint-root-mirror-v1.json"
    ))
    .unwrap()
}
fn root_vector(vectors: &serde_json::Value, epoch: u64, flags: u32) -> Vec<u8> {
    let encoded_epoch = epoch.to_string();
    let vector = vectors["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["epoch"].as_str() == Some(encoded_epoch.as_str()) && v["bankFlags"] == flags)
        .unwrap();
    bytes(&vector["bytes"])
}
fn unchanged(devices: &[Device; 6], images: &[Image; 6]) {
    for (device, image) in devices.iter().zip(images) {
        assert_eq!(device.state.borrow().live.metadata, image.metadata);
        assert_eq!(device.state.borrow().live.pages, image.pages);
    }
}
fn fenced(root: &mut MirroredCheckpointRoot<Device>, trace: &Rc<RefCell<Trace>>) {
    let before = trace.borrow().calls.len();
    assert!(root.new_database_id().is_err());
    assert!(root.data_length().is_err());
    assert!(root.read_metadata(0).is_err());
    assert!(root.write_metadata(0, &[0; 128]).is_err());
    assert!(root.sync_metadata(0).is_err());
    assert!(root.sync_data().is_err());
    assert!(root.truncate_data(0).is_err());
    assert!(root.read_page(0).is_err());
    assert!(root.write_page(0, &[0; 4096]).is_err());
    assert!(root.complete_recovery().is_err());
    assert_eq!(trace.borrow().calls.len(), before);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn independent_root_mode_vectors_preserve_full_epochs_and_owned_bytes() {
    for vector in root_vectors()["decisions"].as_array().unwrap() {
        let (mut devices, trace) = devices::<2>(Default::default());
        let slot = vector["slot"].as_u64().unwrap() as usize;
        let expected = bytes(&vector["bytes"]);
        for device in &devices {
            device.state.borrow_mut().live.metadata[slot] = Some(expected.clone());
        }
        let mut root = MirroredCheckpointRoot::new(
            devices[0].clone(),
            devices[1].clone(),
            vector["bankFlags"].as_u64().unwrap() as u32,
        )
        .unwrap();
        assert_eq!(
            root.metadata_flags(),
            2 | vector["bankFlags"].as_u64().unwrap() as u32
        );
        assert!(root.supports_durable_flush());
        let mut selected = root.read_metadata(slot).unwrap().unwrap();
        assert_eq!(selected, expected);
        selected[0] ^= 1;
        assert_eq!(root.read_metadata(slot).unwrap().unwrap(), expected);
        trace.borrow_mut().calls.clear();
        root.complete_recovery().unwrap();
        assert!(trace.borrow().calls.is_empty());
        devices[1].durable_flush = false;
        let weaker =
            MirroredCheckpointRoot::new(devices[0].clone(), devices[1].clone(), 0).unwrap();
        assert!(!weaker.supports_durable_flush());
        let extracted = root.into_devices();
        assert_eq!(extracted.len(), 2);
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn every_root_byte_tear_preserves_the_acknowledged_epoch_and_following_groups() {
    for flags in [0, 1] {
        let (mut log, devices, _) = root_setup(Default::default(), flags);
        log.append_log_entry("old").unwrap();
        log.flush_log().unwrap();
        log.replace_log_entries(&["acknowledged".into()]).unwrap();
        let source = durable(&devices);
        for side in [0, 5] {
            for byte in 0..128 {
                let mut image = source.clone();
                image[side].metadata[1].as_mut().unwrap()[byte] ^= 1;
                let (mut recovered, repaired, _) = root_setup(image, flags);
                assert_eq!(recovered.epoch(), 1);
                assert_eq!(recovered.read_log_entries().unwrap(), ["acknowledged"]);
                let repaired = durable(&repaired);
                assert_eq!(repaired[0].metadata, repaired[5].metadata);
            }
        }
        for epoch in 2..=4 {
            log.replace_log_entries(&[format!("acknowledged{epoch}")])
                .unwrap();
            let mut image = durable(&devices);
            image[if epoch % 2 == 1 { 0 } else { 5 }].metadata[epoch % 2] = None;
            let (mut recovered, repaired, _) = root_setup(image, flags);
            assert_eq!(recovered.epoch(), epoch as u64);
            recovered.append_log_entry("following").unwrap();
            recovered.flush_log().unwrap();
            recovered
                .replace_log_entries(&["next".into(), "following".into()])
                .unwrap();
            assert_eq!(
                root_setup(durable(&repaired), flags)
                    .0
                    .read_log_entries()
                    .unwrap(),
                ["next", "following"]
            );
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn invalid_or_conflicting_roots_and_selected_banks_never_trigger_repair() {
    let vectors = root_vectors();
    for flags in [0, 1] {
        let (mut log, devices, _) = root_setup(Default::default(), flags);
        log.append_log_entry("old").unwrap();
        log.flush_log().unwrap();
        log.replace_log_entries(&["acknowledged".into()]).unwrap();
        let source = durable(&devices);
        let mut invalid = Vec::new();
        let mut add = |change: &dyn Fn(&mut [Image; 6])| {
            let mut image = source.clone();
            change(&mut image);
            invalid.push(image);
        };
        add(&|i| {
            i[0].metadata[1].as_mut().unwrap()[120] ^= 1;
            i[5].metadata[1].as_mut().unwrap()[120] ^= 1;
        });
        add(&|i| i[5].pages.push([0; 4096]));
        add(&|i| i[0].metadata[0] = Some(root_vector(&vectors, 4, flags)));
        add(&|i| {
            i[0].metadata[0] = Some(root_vector(&vectors, 4, flags));
            i[5].metadata[0] = i[0].metadata[0].clone();
        });
        if flags == 0 {
            for key in [
                "foreign",
                "wrongMode",
                "unknownMode",
                "unknownVersion",
                "nonzeroPadding",
            ] {
                add(&|i| i[0].metadata[1] = Some(bytes(&vectors[key])));
            }
        }
        add(&|i| {
            i[0].metadata[1].as_mut().unwrap()[120] ^= 1;
            i[2].pages[0][56] ^= 1;
            if flags == 1 {
                i[4].pages[0][56] ^= 1;
            }
        });
        add(&|i| {
            i[0].metadata[1].as_mut().unwrap()[120] ^= 1;
            i[2].metadata = [None, None];
            if flags == 1 {
                i[4].metadata = [None, None];
            }
        });
        for image in invalid {
            let (devices, trace) = super::devices(image.clone());
            assert!(root_open(&devices, flags).is_err());
            assert!(
                trace.borrow().calls.iter().all(|c| !c.contains("write")
                    && !c.contains("sync")
                    && !c.contains("truncate")
                    && !c.contains("repair")),
                "{:?}",
                trace.borrow().calls
            );
            unchanged(&devices, &image);
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn root_repair_rechecks_all_sources_before_any_write_and_fences_changed_sources() {
    let (mut log, devices, _) = root_setup(Default::default(), 0);
    log.replace_log_entries(&["acknowledged".into()]).unwrap();
    let mut images = durable(&devices);
    images[0].metadata[0].as_mut().unwrap()[120] ^= 1;
    images[5].metadata[1].as_mut().unwrap()[120] ^= 1;
    let (devices, trace) = super::devices(images);
    let mut root = MirroredCheckpointRoot::new(devices[0].clone(), devices[5].clone(), 0).unwrap();
    root.read_metadata(0).unwrap();
    root.read_metadata(1).unwrap();
    trace.borrow_mut().calls.clear();
    devices[0].state.borrow_mut().live.metadata[1]
        .as_mut()
        .unwrap()[120] ^= 1;
    assert!(root
        .complete_recovery()
        .unwrap_err()
        .to_string()
        .contains("source changed"));
    assert!(trace
        .borrow()
        .calls
        .iter()
        .all(|c| c.contains("readMetadata")));
    fenced(&mut root, &trace);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn every_publication_cut_recovers_complete_old_or_new_and_preserves_acknowledgements() {
    for flags in [0, 1] {
        let (mut log, devices, _) = root_setup(Default::default(), flags);
        log.append_log_entry("old").unwrap();
        log.flush_log().unwrap();
        let images = durable(&devices);
        let replacement = vec!["雪".repeat(700), "\u{feff}\0😀".into(), String::new()];
        let (mut success, _, trace) = root_setup(images.clone(), flags);
        trace.borrow_mut().calls.clear();
        success.replace_log_entries(&replacement).unwrap();
        let operations = trace.borrow().calls.len();
        assert!((18..=80).contains(&operations));
        assert_eq!(
            &trace.borrow().calls[operations - 4..],
            [
                "root.writeMetadata1",
                "rootMirror.writeMetadata1",
                "root.syncMetadata1",
                "rootMirror.syncMetadata1"
            ]
        );
        for at in 1..=operations {
            for after in [false, true] {
                let (mut log, devices, trace) = root_setup(images.clone(), flags);
                trace.borrow_mut().calls.clear();
                trace.borrow_mut().fault = Some((at, after));
                let acknowledged = match log.replace_log_entries(&replacement) {
                    Ok(replaced) => replaced,
                    Err(error) => {
                        assert!(error.to_string().contains("injected"));
                        let count = trace.borrow().calls.len();
                        assert!(log.read_log_entries().is_err());
                        assert!(log.append_log_entry("bad").is_err());
                        assert!(log.flush_log().is_err());
                        assert!(log.replace_log_entries(&["bad".into()]).is_err());
                        assert_eq!(trace.borrow().calls.len(), count);
                        false
                    }
                };
                let (mut recovered, repaired, _) = root_setup(durable(&devices), flags);
                let found = recovered.read_log_entries().unwrap();
                if acknowledged {
                    assert_eq!(found, replacement);
                } else {
                    assert!(found == ["old"] || found == replacement);
                }
                recovered.append_log_entry("following").unwrap();
                recovered.flush_log().unwrap();
                let following = [found, vec!["following".into(), "compacted".into()]].concat();
                recovered.replace_log_entries(&following).unwrap();
                assert_eq!(
                    root_setup(durable(&repaired), flags)
                        .0
                        .read_log_entries()
                        .unwrap(),
                    following
                );
            }
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn every_root_repair_cut_preserves_acknowledged_epoch_and_bank_reuse() {
    for flags in [0, 1] {
        for side in [0, 5] {
            let (mut log, devices, _) = root_setup(Default::default(), flags);
            log.append_log_entry("old").unwrap();
            log.flush_log().unwrap();
            log.replace_log_entries(&["acknowledged".into()]).unwrap();
            let mut images = durable(&devices);
            images[side].metadata[1].as_mut().unwrap()[120] ^= 1;
            let (_, _, trace) = root_setup(images.clone(), flags);
            let operations = trace.borrow().calls.len();
            assert!((12..=80).contains(&operations));
            for at in 1..=operations {
                for after in [false, true] {
                    let (devices, trace) = super::devices(images.clone());
                    trace.borrow_mut().fault = Some((at, after));
                    match root_open(&devices, flags) {
                        Ok(mut log) => {
                            assert_eq!(log.read_log_entries().unwrap(), ["acknowledged"])
                        }
                        Err(error) => assert!(
                            error.to_string().contains("injected")
                                || error.to_string().contains("unreadable"),
                            "{at}/{after}: {error}"
                        ),
                    }
                    let (mut recovered, repaired, _) = root_setup(durable(&devices), flags);
                    assert_eq!(recovered.epoch(), 1);
                    assert_eq!(recovered.read_log_entries().unwrap(), ["acknowledged"]);
                    recovered.append_log_entry("following").unwrap();
                    recovered.flush_log().unwrap();
                    recovered
                        .replace_log_entries(&["reused".into(), "following".into()])
                        .unwrap();
                    assert_eq!(
                        root_setup(durable(&repaired), flags)
                            .0
                            .read_log_entries()
                            .unwrap(),
                        ["reused", "following"]
                    );
                }
            }
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn root_mode_and_bank_mode_conversions_reject_before_mutation() {
    for flags in [0, 1] {
        let (mut log, devices, _) = root_setup(Default::default(), flags);
        log.replace_log_entries(&["acknowledged".into()]).unwrap();
        let images = durable(&devices);
        let (devices, trace) = super::devices(images.clone());
        assert!(CheckpointPagedLog::open(devices[0].clone(), banks(&devices, flags)).is_err());
        let wrong =
            MirroredCheckpointRoot::new(devices[0].clone(), devices[5].clone(), 1 - flags).unwrap();
        assert!(CheckpointPagedLog::open(wrong, banks(&devices, flags)).is_err());
        unchanged(&devices, &images);
        assert!(trace.borrow().calls.iter().all(|c| !c.contains("write")
            && !c.contains("sync")
            && !c.contains("truncate")
            && !c.contains("repair")));
    }
    let (mut log, devices, _) = setup(Default::default());
    log.replace_log_entries(&["old".into()]).unwrap();
    let [root, a, b] = durable(&devices);
    let images = [
        root,
        a,
        b,
        Image::default(),
        Image::default(),
        Image::default(),
    ];
    let (devices, _) = super::devices(images.clone());
    assert!(root_open(&devices, 0).is_err());
    unchanged(&devices, &images);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn raw_root_contract_rejects_bad_modes_slots_publications_and_data() {
    let (mut devices, trace) = devices::<2>(Default::default());
    assert!(MirroredCheckpointRoot::new(devices[0].clone(), devices[1].clone(), 2).is_err());
    devices[1].flags = 1;
    assert!(MirroredCheckpointRoot::new(devices[0].clone(), devices[1].clone(), 0).is_err());
    devices[1].flags = 0;
    let mut root = MirroredCheckpointRoot::new(devices[0].clone(), devices[1].clone(), 0).unwrap();
    assert_eq!(
        root.new_database_id().unwrap(),
        std::array::from_fn(|i| i as u8)
    );
    trace.borrow_mut().calls.clear();
    for slot in [2, usize::MAX] {
        assert!(root.read_metadata(slot).is_err());
        assert!(root.write_metadata(slot, &[0; 128]).is_err());
        assert!(root.sync_metadata(slot).is_err());
    }
    assert!(root.write_metadata(0, &[0; 128]).is_err());
    assert!(root.read_page(0).is_err());
    assert!(root.write_page(0, &[0; 4096]).is_err());
    assert!(root.truncate_data(1).is_err());
    assert!(trace.borrow().calls.is_empty());
    root.truncate_data(0).unwrap();
    assert_eq!(
        trace.borrow().calls,
        ["root.truncateData", "bank0.truncateData"]
    );
    devices[0].state.borrow_mut().live.pages.push([0; 4096]);
    assert!(root.data_length().is_err());
    devices[0].state.borrow_mut().live.pages.clear();
    devices[1].state.borrow_mut().live.pages.push([0; 4096]);
    assert!(root.data_length().is_err());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn incompatible_empty_root_and_bank_modes_reject_before_initialization() {
    let (devices, trace) = devices::<6>(Default::default());
    let root = MirroredCheckpointRoot::new(devices[0].clone(), devices[5].clone(), 1).unwrap();
    assert!(CheckpointPagedLog::open(root, banks(&devices, 0)).is_err());
    assert!(trace.borrow().calls.is_empty());
    unchanged(&devices, &Default::default());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn raw_root_mutation_failures_fence_without_retry_and_empty_selections_clear_repairs() {
    let encoded: [u8; 128] = root_vector(&root_vectors(), 0, 0).try_into().unwrap();
    for method in 0..4 {
        for after in [false, true] {
            let (devices, trace) = devices::<2>(Default::default());
            let mut root =
                MirroredCheckpointRoot::new(devices[0].clone(), devices[1].clone(), 0).unwrap();
            trace.borrow_mut().fault = Some((1, after));
            let result = match method {
                0 => root.sync_data(),
                1 => root.truncate_data(0),
                2 => root.sync_metadata(0),
                _ => root.write_metadata(0, &encoded),
            };
            assert!(result.unwrap_err().to_string().contains("injected"));
            fenced(&mut root, &trace);
        }
    }
    let (devices, trace) = devices::<2>(Default::default());
    let mut root = MirroredCheckpointRoot::new(devices[0].clone(), devices[1].clone(), 0).unwrap();
    devices[0].state.borrow_mut().live.metadata[0] = Some(encoded.to_vec());
    assert_eq!(root.read_metadata(0).unwrap().unwrap(), encoded);
    for device in &devices {
        device.state.borrow_mut().live.metadata[0] = Some(Vec::new());
    }
    assert!(root.read_metadata(0).unwrap().is_none());
    trace.borrow_mut().calls.clear();
    root.complete_recovery().unwrap();
    assert!(trace.borrow().calls.is_empty());
}
