//! Finite durable-image harness shared by native and actual-WASM store controls.
use link_cli::{
    engine::{lsm::LsmTableNamespace, paged::PageDevice},
    LinkError,
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
#[derive(Default)]
struct Effects {
    trace: Vec<String>,
    fault: Option<(usize, bool)>,
}
fn injected(name: &str) -> LinkError {
    LinkError::StorageError(format!("injected {name}"))
}
fn effect(control: &Rc<RefCell<Effects>>, name: String) -> Result<bool, LinkError> {
    let mut state = control.borrow_mut();
    state.trace.push(name.clone());
    match state.fault.filter(|(at, _)| *at == state.trace.len()) {
        Some((_, false)) => Err(injected(&name)),
        Some((_, true)) => Ok(true),
        None => Ok(false),
    }
}
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub(super) struct Image {
    metadata: [Option<Vec<u8>>; 2],
    pages: Vec<[u8; 4096]>,
}
#[derive(Default)]
pub(super) struct State {
    live: Image,
    durable: Image,
    calls: Vec<String>,
    reads: usize,
    pub(super) completed: usize,
    pub(super) non_durable: bool,
    mode: u32,
    fault: Option<(usize, bool)>,
}
#[derive(Clone, Default)]
pub(super) struct Device(
    pub(super) Rc<RefCell<State>>,
    Option<(Rc<RefCell<Effects>>, String)>,
);
impl Device {
    fn from_image(image: Image) -> Self {
        Self(
            Rc::new(RefCell::new(State {
                live: image.clone(),
                durable: image,
                ..State::default()
            })),
            None,
        )
    }
    fn controlled(image: Image, control: Rc<RefCell<Effects>>, name: String) -> Self {
        let mut device = Self::from_image(image);
        device.1 = Some((control, name));
        device
    }
    fn action(&self, name: String, body: impl FnOnce(&mut State)) -> Result<(), LinkError> {
        let global = if let Some((control, device)) = &self.1 {
            effect(control, format!("{device}:{name}"))?
        } else {
            false
        };
        let mut s = self.0.borrow_mut();
        s.calls.push(name.clone());
        let fault = s.fault.filter(|(at, _)| *at == s.calls.len());
        if matches!(fault, Some((_, false))) {
            return Err(LinkError::StorageError(format!("injected {name}")));
        }
        body(&mut s);
        if fault.is_some() || global {
            // A complete unsynced decision may persist before an error reply.
            if name.starts_with("writeMetadata") {
                s.durable.metadata = s.live.metadata.clone();
            }
            return Err(LinkError::StorageError(format!("injected {name}")));
        }
        Ok(())
    }
    pub(super) fn crash(&self) -> Self {
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
        !self.0.borrow().non_durable
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
pub(super) struct Tables(pub(super) Rc<RefCell<TableState>>);
#[derive(Default)]
pub(super) struct TableState {
    pub(super) files: BTreeMap<u64, Device>,
    pub(super) leases: BTreeSet<u64>,
    pub(super) mutations: Vec<String>,
    control: Rc<RefCell<Effects>>,
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
        let lost = effect(&state.control, format!("create:{id}"))?;
        let device = Device::controlled(
            Image::default(),
            state.control.clone(),
            format!("table-{id}"),
        );
        state.files.insert(id, device.clone());
        assert!(state.leases.insert(id));
        state.mutations.push("create".into());
        if lost {
            state.leases.remove(&id);
            return Err(injected("create"));
        }
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
        let lost = effect(&state.control, format!("close:table-{id}"))?;
        state.leases.remove(&id); // Close remains idempotent after a lost reply.
        state.mutations.push("close".into());
        if lost {
            return Err(injected("close"));
        }
        Ok(())
    }
    fn delete(&mut self, id: u64) -> Result<(), LinkError> {
        let mut state = self.0.borrow_mut();
        assert!(!state.leases.contains(&id));
        let lost = effect(&state.control, format!("delete:{id}"))?;
        assert!(state.files.remove(&id).is_some());
        state.mutations.push("delete".into());
        if lost {
            return Err(injected("delete"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Snapshot {
    wal: [Image; 3],
    tables: BTreeMap<u64, Image>,
}
pub(super) struct Harness {
    pub devices: [Device; 3],
    pub tables: Tables,
    control: Rc<RefCell<Effects>>,
}
impl Harness {
    pub fn new() -> Self {
        Self::from_image(Snapshot {
            wal: std::array::from_fn(|_| Image::default()),
            tables: BTreeMap::new(),
        })
    }
    pub fn from_image(image: Snapshot) -> Self {
        let control = Rc::new(RefCell::new(Effects::default()));
        let devices = image
            .wal
            .into_iter()
            .enumerate()
            .map(|(i, image)| {
                Device::controlled(image, control.clone(), ["root", "bank0", "bank1"][i].into())
            })
            .collect::<Vec<_>>()
            .try_into()
            .ok()
            .unwrap();
        let tables = Tables(Rc::new(RefCell::new(TableState {
            files: image
                .tables
                .into_iter()
                .map(|(id, image)| {
                    (
                        id,
                        Device::controlled(image, control.clone(), format!("table-{id}")),
                    )
                })
                .collect(),
            control: control.clone(),
            ..TableState::default()
        })));
        Self {
            devices,
            tables,
            control,
        }
    }
    pub fn image(&self) -> Snapshot {
        Snapshot {
            wal: std::array::from_fn(|i| self.devices[i].0.borrow().durable.clone()),
            tables: self
                .tables
                .0
                .borrow()
                .files
                .iter()
                .map(|(id, d)| (*id, d.0.borrow().durable.clone()))
                .collect(),
        }
    }
    pub fn reset(&self) {
        self.control.borrow_mut().trace.clear();
        self.control.borrow_mut().fault = None;
    }
    pub fn trace(&self) -> Vec<String> {
        self.control.borrow().trace.clone()
    }
    pub fn fault(&self, at: usize, after: bool) {
        self.control.borrow_mut().fault = Some((at, after));
    }
    pub fn wal(
        &self,
    ) -> Result<link_cli::engine::paged::CheckpointPagedLog<Device, Device>, LinkError> {
        link_cli::engine::paged::CheckpointPagedLog::with_max_pages(
            self.devices[0].clone(),
            [self.devices[1].clone(), self.devices[2].clone()],
            128,
        )
    }
}
