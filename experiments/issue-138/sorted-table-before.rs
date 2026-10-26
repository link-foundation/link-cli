use super::*;
use link_cli::engine::paged::{SortedJournalEntry, SortedJournalTable, SortedTableOptions};
fn rows() -> Vec<SortedJournalEntry> {
    vec![
        SortedJournalEntry { key: "a".into(), generation: 1, entries: Some(vec!["\u{feff}雪\0😀\n".into()]) },
        SortedJournalEntry { key: "b".into(), generation: u64::MAX, entries: None },
        SortedJournalEntry { key: "c".into(), generation: 3, entries: Some(vec![]) },
    ]
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn sealed_sorted_table_roundtrip_tombstones_and_owned_values() {
    let device = Device::default();
    let mut table = SortedJournalTable::create(device.clone(), &rows(), Default::default()).unwrap();
    assert_eq!(table.len(), 3); assert!(!table.is_empty()); assert!(table.supports_durable_flush());
    assert_eq!(table.get("a").unwrap(), Some(rows()[0].clone()));
    assert_eq!(table.get("b").unwrap(), Some(rows()[1].clone()));
    assert_eq!(table.get("c").unwrap(), Some(rows()[2].clone()));
    assert_eq!(table.get("absent").unwrap(), None);
    assert_eq!(table.entries().unwrap(), rows());
    let mut owned = table.get("a").unwrap().unwrap(); owned.entries.as_mut().unwrap()[0] = "caller".into();
    assert_eq!(table.get("a").unwrap(), Some(rows()[0].clone()));
    assert_eq!(SortedJournalTable::open(device.crash(), Default::default()).unwrap().entries().unwrap(), rows());
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn table_rejects_unsorted_duplicate_and_zero_generation_before_io() {
    for input in [vec![rows()[1].clone(), rows()[0].clone()], vec![rows()[0].clone();2], vec![SortedJournalEntry{generation:0,..rows()[0].clone()}]] {
        let device = Device::default();
        assert!(SortedJournalTable::create(device.clone(), &input, Default::default()).is_err());
        assert!(device.0.borrow().calls.is_empty()); assert_eq!(device.0.borrow().live, Image::default());
    }
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn table_point_lookup_reads_selected_pages_and_detects_post_open_damage() {
    let input: Vec<_> = (0..80).map(|i| SortedJournalEntry { key: format!("k{i:03}"), generation:i+1, entries:Some(vec!["x".repeat(1000)]) }).collect();
    let device=Device::default();
    let mut table=SortedJournalTable::create(device.clone(),&input,Default::default()).unwrap();
    assert!(device.0.borrow().live.pages.len()>40);
    device.0.borrow_mut().calls.clear();
    assert_eq!(table.get("k040").unwrap(),Some(input[40].clone()));
    assert!(device.0.borrow().calls.len()<=3);
    let end=device.0.borrow().live.pages.len()-1;
    device.0.borrow_mut().live.pages[end][64]^=1;
    assert!(table.get("k079").is_err());
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn table_limits_and_existing_data_are_preserved() {
    let device=Device::default();
    assert!(SortedJournalTable::create(device.clone(),&rows(),SortedTableOptions{max_keys:2,..Default::default()}).is_err());
    assert!(device.0.borrow().calls.is_empty());
    SortedJournalTable::create(device.clone(),&rows(),Default::default()).unwrap();
    let before=device.0.borrow().live.clone();
    assert!(SortedJournalTable::create(device.clone(),&rows(),Default::default()).is_err());
    assert_eq!(device.0.borrow().live,before);
    assert!(SortedJournalTable::open(device.clone(),SortedTableOptions{max_bytes:8,..Default::default()}).is_err());
    assert_eq!(device.0.borrow().live,before);
}
