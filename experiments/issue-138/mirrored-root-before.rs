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
