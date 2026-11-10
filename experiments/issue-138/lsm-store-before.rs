// Physical composition controls prepared before the missing API exists.
use link_cli::engine::cluster::{AtomicJournalStore, JournalVersion};
use link_cli::engine::lsm::{
    LsmOptions, LsmStoreOptions, LsmTableNamespace, PhysicalLsmJournalStore,
};
use link_cli::engine::maintenance::{JournalMaintenanceStore, JournalScanOptions};
use link_cli::engine::paged::{CheckpointPagedLog, PageDevice};
use link_cli::{LinkError, TransitionLogStore};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
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
    reads: usize,
    completed: usize,
    mode: u32,
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
    fn metadata_flags(&self) -> u32 {
        self.0.borrow().mode
    }
    fn complete_recovery(&mut self) -> Result<(), LinkError> {
        self.0.borrow_mut().completed += 1;
        Ok(())
    }
    fn supports_durable_flush(&self) -> bool {
        true
    }
    fn new_database_id(&mut self) -> Result<[u8; 16], LinkError> {
        self.0.borrow_mut().reads += 1;
        Ok(std::array::from_fn(|i| i as u8))
    }
    fn data_length(&mut self) -> Result<u64, LinkError> {
        self.0.borrow_mut().reads += 1;
        Ok(self.0.borrow().live.pages.len() as u64 * 4096)
    }
    fn read_metadata(&mut self, slot: usize) -> Result<Option<Vec<u8>>, LinkError> {
        self.0.borrow_mut().reads += 1;
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
impl link_cli::engine::paged::RepairPageDevice for Device {
    fn repair_page(&mut self, index: u64, bytes: &[u8; 4096]) -> Result<(), LinkError> {
        self.action("repairPage".into(), |s| {
            if index as usize == s.live.pages.len() {
                s.live.pages.push(*bytes);
            } else {
                s.live.pages[index as usize] = *bytes;
            }
        })
    }
}
#[derive(Clone, Default)]
struct Tables(Rc<RefCell<TableState>>);
#[derive(Default)]
struct TableState {
    files: BTreeMap<u64, Device>,
    leases: BTreeSet<u64>,
    mutations: Vec<String>,
}
impl LsmTableNamespace for Tables {
    type Device = Device;
    fn supports_durable_flush(&self) -> bool {
        true
    }
    fn list(&mut self, max_entries: usize) -> Result<Vec<u64>, LinkError> {
        let state = self.0.borrow();
        assert!(state.files.len() <= max_entries);
        Ok(state.files.keys().copied().collect())
    }
    fn create(&mut self, id: u64) -> Result<Device, LinkError> {
        let mut state = self.0.borrow_mut();
        assert!(!state.files.contains_key(&id));
        let device = Device::default();
        state.files.insert(id, device.clone());
        assert!(state.leases.insert(id));
        state.mutations.push("create".into());
        Ok(device)
    }
    fn open(&mut self, id: u64) -> Result<Device, LinkError> {
        let mut state = self.0.borrow_mut();
        let device = state.files.get(&id).unwrap().clone();
        assert!(state.leases.insert(id));
        Ok(device)
    }
    fn close(&mut self, device: &mut Device) -> Result<(), LinkError> {
        let mut state = self.0.borrow_mut();
        let id = *state
            .files
            .iter()
            .find(|(_, value)| Rc::ptr_eq(&value.0, &device.0))
            .unwrap()
            .0;
        assert!(state.leases.remove(&id));
        state.mutations.push("close".into());
        Ok(())
    }
    fn delete(&mut self, id: u64) -> Result<(), LinkError> {
        let mut state = self.0.borrow_mut();
        assert!(!state.leases.contains(&id));
        assert!(state.files.remove(&id).is_some());
        state.mutations.push("delete".into());
        Ok(())
    }
}
fn options() -> LsmStoreOptions {
    LsmStoreOptions {
        limits: LsmOptions {
            max_keys: 32,
            max_bytes: 4096,
            buffer_keys: 2,
            buffer_bytes: 128,
            max_runs: 2,
            max_wal_records: 4,
            max_wal_bytes: 8192,
        },
        max_namespace_entries: 16,
    }
}
fn put<S: AtomicJournalStore>(
    store: &mut S,
    key: &str,
    entries: Vec<String>,
) -> Result<(), LinkError> {
    let keys = vec![key.to_owned()];
    let before = store.read(&keys)?;
    assert!(store.compare_exchange(&keys, &before, &[Some(entries)])?);
    Ok(())
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn physical_lsm_group_owned_tombstone_empty_and_reopen() -> Result<(), LinkError> {
    let devices = [Device::default(), Device::default(), Device::default()];
    let wal = CheckpointPagedLog::with_max_pages(
        devices[0].clone(),
        [devices[1].clone(), devices[2].clone()],
        128,
    )?;
    let tables = Tables::default();
    let mut store = PhysicalLsmJournalStore::open(wal, tables.clone(), options())?;
    let keys = vec!["a".into(), "b".into()];
    let absent = store.read(&keys)?;
    assert_eq!(absent, vec![JournalVersion::default(); 2]);
    assert!(store.compare_exchange(
        &keys,
        &absent,
        &[Some(vec!["\u{feff}雪\0😀\n".into()]), Some(vec![])]
    )?);
    let mut first = store.read(&keys)?;
    assert!(first[0].generation > 0);
    assert_eq!(first[0].generation, first[1].generation);
    first[0].entries[0] = "caller".into();
    assert_eq!(store.read(&keys)?[0].entries[0], "\u{feff}雪\0😀\n");
    store.flush()?;
    let count = tables.0.borrow().mutations.len();
    assert!(!store.compare_exchange(&keys, &absent, &[None, Some(vec!["lost".into()])])?);
    let current = store.read(&keys)?;
    assert!(store.compare_exchange(
        &keys,
        &current,
        &[Some(current[0].entries.clone()), Some(vec![])]
    )?);
    assert_eq!(tables.0.borrow().mutations.len(), count);
    assert!(store.compare_delete("a", current[0].generation)?);
    let deleted = store.read(&keys)?[0].clone();
    assert!(deleted.generation > current[0].generation);
    assert!(deleted.entries.is_empty());
    assert!(!store.compare_delete("a", deleted.generation)?);
    assert_eq!(
        store
            .scan_namespace(JournalScanOptions::default())?
            .iter()
            .map(|r| r.key.as_str())
            .collect::<Vec<_>>(),
        ["b"]
    );
    put(&mut store, "a", vec![])?;
    store.compact()?;
    let expected = store.read(&keys)?;
    store.close()?;
    assert!(tables.0.borrow().leases.is_empty());
    let fresh = Tables::default();
    for (id, device) in &tables.0.borrow().files {
        fresh.0.borrow_mut().files.insert(*id, device.crash());
    }
    let wal = CheckpointPagedLog::with_max_pages(
        devices[0].crash(),
        [devices[1].crash(), devices[2].crash()],
        128,
    )?;
    let mut reopened = PhysicalLsmJournalStore::open(wal, fresh, options())?;
    assert_eq!(reopened.read(&keys)?, expected);
    reopened.close()?;
    Ok(())
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn physical_lsm_repeated_buffer_and_wal_compaction() -> Result<(), LinkError> {
    let wal = CheckpointPagedLog::with_max_pages(
        Device::default(),
        [Device::default(), Device::default()],
        128,
    )?;
    let tables = Tables::default();
    let mut store = PhysicalLsmJournalStore::open(wal, tables.clone(), options())?;
    for i in 0..30 {
        put(&mut store, &format!("k{}", i % 5), vec![i.to_string()])?;
        if i % 4 == 0 {
            store.flush()?;
        }
    }
    store.compact()?;
    assert_eq!(tables.0.borrow().files.len(), 1);
    assert!(tables.0.borrow().mutations.iter().any(|s| s == "delete"));
    assert_eq!(store.read(&["k2".into()])?[0].entries, ["27"]);
    assert!(put(&mut store, "oversized", vec!["x".repeat(129)]).is_err());
    put(&mut store, "following", vec!["ok".into()])?;
    store.flush()?;
    let (mut wal, _) = store.into_parts()?;
    assert!(wal.read_log_entries()?.len() <= 4);
    Ok(())
}
