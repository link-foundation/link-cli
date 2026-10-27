use super::*;
use link_cli::engine::paged::MirroredCheckpointRoot;

fn inspect(source: &mut [Device; 3], max: u64) -> Result<Vec<String>, LinkError> {
    let [root, first, second] = source;
    CheckpointPagedLog::inspect(root, &mut [first.clone(), second.clone()], max)
}
fn unchanged<const N: usize>(
    source: &[Device; N],
    images: &[Image; N],
    trace: &Rc<RefCell<Trace>>,
) {
    assert!(trace
        .borrow()
        .calls
        .iter()
        .all(|s| s.contains("read") || s.ends_with("dataLength")));
    for i in 0..N {
        let state = source[i].state.borrow();
        assert_eq!(state.live.metadata, images[i].metadata);
        assert_eq!(state.live.pages, images[i].pages);
        assert_eq!(state.durable.metadata, images[i].metadata);
        assert_eq!(state.durable.pages, images[i].pages);
    }
}
fn fixture() -> [Image; 3] {
    let (mut log, source, _) = setup(Default::default());
    log.append_log_entry("old").unwrap();
    log.flush_log().unwrap();
    log.replace_log_entries(&["acknowledged\0\u{feff}😀".into()])
        .unwrap();
    durable(&source)
}
fn sign(bytes: &mut [u8]) {
    let sum = bytes[..120].iter().fold(0xcbf29ce484222325_u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    });
    bytes[120..].copy_from_slice(&sum.to_le_bytes());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn empty_partial_initialization_limits_and_modes_never_allocate_or_write() {
    let (mut source, trace) = devices::<3>(Default::default());
    assert!(inspect(&mut source, 4).unwrap().is_empty());
    unchanged(&source, &Default::default(), &trace);
    let mut images = fixture();
    images[0].metadata = [None, None];
    for bank in &mut images[1..] {
        bank.pages.clear();
        bank.metadata[1] = None;
    }
    let (mut source, trace) = devices(images.clone());
    assert!(inspect(&mut source, 4).unwrap().is_empty());
    unchanged(&source, &images, &trace);
    for max in [0, u64::MAX] {
        trace.borrow_mut().calls.clear();
        assert!(inspect(&mut source, max).is_err());
        assert!(trace.borrow().calls.is_empty());
    }
    for flags in [[1, 0, 0], [2, 1, 1], [0, 2, 2], [0, 0, 1]] {
        let (mut source, trace) = devices::<3>(Default::default());
        for i in 0..3 {
            source[i].flags = flags[i];
        }
        assert!(inspect(&mut source, 4).is_err());
        assert!(trace.borrow().calls.is_empty());
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn selected_prefix_is_owned_ignores_inactive_damage_and_preserves_suffix_until_open() {
    let mut images = fixture();
    images[2].pages.push([7; 4096]);
    images[1].metadata = [Some(vec![19; 128]), None];
    images[1].pages[0][20] ^= 1;
    let (mut source, trace) = devices(images.clone());
    let mut records = inspect(&mut source, 4).unwrap();
    assert_eq!(records, ["acknowledged\0\u{feff}😀"]);
    records[0].clear();
    assert_eq!(
        inspect(&mut source, 4).unwrap(),
        ["acknowledged\0\u{feff}😀"]
    );
    unchanged(&source, &images, &trace);
    let mut log = open(&source).unwrap();
    assert_eq!(
        log.read_log_entries().unwrap(),
        ["acknowledged\0\u{feff}😀"]
    );
    assert_eq!(source[2].state.borrow().live.pages.len(), 1);
    assert!(trace
        .borrow()
        .calls
        .iter()
        .any(|s| s.ends_with("truncateData")));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn corrupt_selected_decisions_pages_roots_and_unrooted_commits_preserve_evidence() {
    let images = fixture();
    let mut cases = Vec::new();
    let mut bad = images.clone();
    bad[2].metadata = [None, None];
    cases.push(bad);
    let mut bad = images.clone();
    bad[2].metadata = images[1].metadata.clone();
    cases.push(bad);
    let mut bad = images.clone();
    bad[2].pages[0][56] ^= 1;
    cases.push(bad);
    let mut bad = images.clone();
    bad[2].pages.clear();
    cases.push(bad);
    let mut bad = images.clone();
    bad[0].pages.push([0; 4096]);
    cases.push(bad);
    let mut bad = images.clone();
    bad[0].metadata = [None, None];
    cases.push(bad);
    let mut bad = images.clone();
    bad[0].metadata = [None, None];
    bad[1].pages.clear();
    bad[2].pages.clear();
    cases.push(bad);
    let mut bad = images.clone();
    let mut raw = bad[2].metadata[1].clone().unwrap();
    raw[72] ^= 1;
    sign(&mut raw);
    bad[2].metadata[0] = Some(raw);
    cases.push(bad);
    for root in vectors()["invalid"].as_array().unwrap() {
        let mut bad = images.clone();
        bad[0].metadata[0] = Some(bytes(root));
        cases.push(bad);
    }
    let mut bad = images.clone();
    bad[0].metadata[1] = Some(bytes(&vectors()["foreignDecision"]));
    cases.push(bad);
    for image in cases {
        let (mut source, trace) = devices(image.clone());
        assert!(inspect(&mut source, 4).is_err());
        unchanged(&source, &image, &trace);
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn lower_page_bound_rejects_without_truncating_selected_data() {
    let (mut log, source, _) = setup(Default::default());
    log.append_log_entry(&"x".repeat(2000)).unwrap();
    log.flush_log().unwrap();
    let images = durable(&source);
    let (mut source, trace) = devices(images.clone());
    assert!(inspect(&mut source, 1)
        .unwrap_err()
        .to_string()
        .contains("exceed selected"));
    unchanged(&source, &images, &trace);
    assert_eq!(inspect(&mut source, 4).unwrap(), ["x".repeat(2000)]);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn every_inspection_read_error_keeps_borrowed_owners_and_permits_retry() {
    let images = fixture();
    let (mut source, trace) = devices(images.clone());
    inspect(&mut source, 4).unwrap();
    let operations = trace.borrow().calls.len();
    assert!((6..=20).contains(&operations));
    for at in 1..=operations {
        for after in [false, true] {
            let (mut source, trace) = devices(images.clone());
            trace.borrow_mut().fault = Some((at, after));
            assert!(inspect(&mut source, 4).is_err());
            unchanged(&source, &images, &trace);
            trace.borrow_mut().fault = None;
            assert_eq!(
                inspect(&mut source, 4).unwrap(),
                ["acknowledged\0\u{feff}😀"]
            );
            unchanged(&source, &images, &trace);
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn mirrored_root_and_banks_stage_repairs_until_later_validated_opening() {
    let wrap = |source: &[Device; 6]| {
        (
            MirroredCheckpointRoot::new(source[0].clone(), source[5].clone(), 1).unwrap(),
            [
                MirroredPageDevice::new(source[1].clone(), source[3].clone()),
                MirroredPageDevice::new(source[2].clone(), source[4].clone()),
            ],
        )
    };
    let (source, _) = devices::<6>(Default::default());
    let (root, banks) = wrap(&source);
    let mut log = CheckpointPagedLog::with_max_pages(root, banks, 4).unwrap();
    log.append_log_entry("old").unwrap();
    log.flush_log().unwrap();
    log.replace_log_entries(&["acknowledged".into()]).unwrap();
    let mut images = durable(&source);
    images[0].metadata[1].as_mut().unwrap()[120] ^= 1;
    images[2].pages[0][100] ^= 1;
    let (source, trace) = devices(images.clone());
    let (mut root, mut banks) = wrap(&source);
    assert_eq!(
        CheckpointPagedLog::inspect(&mut root, &mut banks, 4).unwrap(),
        ["acknowledged"]
    );
    unchanged(&source, &images, &trace);
    drop(root);
    drop(banks);
    let (root, banks) = wrap(&source);
    let mut log = CheckpointPagedLog::with_max_pages(root, banks, 4).unwrap();
    assert_eq!(log.read_log_entries().unwrap(), ["acknowledged"]);
    assert_eq!(
        source[0].state.borrow().live.metadata,
        source[5].state.borrow().live.metadata
    );
    assert_eq!(
        source[2].state.borrow().live.pages,
        source[4].state.borrow().live.pages
    );
}
