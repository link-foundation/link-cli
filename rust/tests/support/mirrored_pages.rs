//! Finite durable-image pair; shared trace orders mutations across both copies.
use link_cli::engine::paged::{MirroredPageDevice, PageDevice, PagedLog, RepairPageDevice};
use link_cli::{LinkError, TransitionLogStore};
use std::{cell::RefCell, rc::Rc};
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct Image {
    pub metadata: [Option<Vec<u8>>; 2],
    pub pages: Vec<[u8; 4096]>,
}
#[derive(Default)]
pub struct Trace {
    pub calls: Vec<String>,
    pub reads: Vec<String>,
    pub fault: Option<(usize, bool)>,
}
#[derive(Default)]
pub struct ReadFaults {
    pub page: bool,
    pub metadata: bool,
    pub length: Option<u64>,
    pub change_source: bool,
    pub zero_reads: usize,
    pub volatile: bool,
}
#[derive(Clone)]
pub struct Device {
    pub state: Rc<RefCell<(Image, Image)>>,
    pub trace: Rc<RefCell<Trace>>,
    pub side: usize,
    pub faults: Rc<RefCell<ReadFaults>>,
}
impl Device {
    pub fn image(&self) -> Image {
        self.state.borrow().0.clone()
    }
    pub fn durable(&self) -> Image {
        self.state.borrow().1.clone()
    }
    fn action(&self, name: &str, body: impl FnOnce(&mut (Image, Image))) -> Result<(), LinkError> {
        let mut trace = self.trace.borrow_mut();
        trace.calls.push(format!("{}:{name}", self.side));
        let fault = trace.fault.filter(|(at, _)| *at == trace.calls.len());
        if fault == Some((trace.calls.len(), false)) {
            return Err(error("injected before"));
        }
        let mut state = self.state.borrow_mut();
        body(&mut state);
        if fault.is_some() {
            if name.starts_with("writeMetadata") {
                state.1.metadata = state.0.metadata.clone();
            }
            return Err(error("injected after"));
        }
        Ok(())
    }
}
pub fn error(message: &str) -> LinkError {
    LinkError::StorageError(message.into())
}
impl PageDevice for Device {
    fn supports_durable_flush(&self) -> bool {
        !self.faults.borrow().volatile
    }
    fn new_database_id(&mut self) -> Result<[u8; 16], LinkError> {
        Ok(std::array::from_fn(|i| i as u8))
    }
    fn data_length(&mut self) -> Result<u64, LinkError> {
        self.trace
            .borrow_mut()
            .reads
            .push(format!("{}:length", self.side));
        Ok(self
            .faults
            .borrow()
            .length
            .unwrap_or(self.image().pages.len() as u64 * 4096))
    }
    fn read_metadata(&mut self, slot: usize) -> Result<Option<Vec<u8>>, LinkError> {
        self.trace
            .borrow_mut()
            .reads
            .push(format!("{}:metadata", self.side));
        if self.faults.borrow().metadata {
            return Err(error("injected metadata read"));
        }
        Ok(self.image().metadata[slot].clone())
    }
    fn read_page(&mut self, index: u64) -> Result<[u8; 4096], LinkError> {
        self.trace
            .borrow_mut()
            .reads
            .push(format!("{}:page", self.side));
        let mut faults = self.faults.borrow_mut();
        if faults.page {
            return Err(error("injected page read"));
        }
        if index == 0 {
            faults.zero_reads += 1;
            if faults.change_source && faults.zero_reads == 2 {
                self.state.borrow_mut().0.pages[0][64] ^= 1;
            }
        }
        self.image()
            .pages
            .get(index as usize)
            .copied()
            .ok_or_else(|| error("short page"))
    }
    fn write_page(&mut self, index: u64, bytes: &[u8; 4096]) -> Result<(), LinkError> {
        self.action("writePage", |s| {
            assert_eq!(index as usize, s.0.pages.len());
            s.0.pages.push(*bytes);
        })
    }
    fn sync_data(&mut self) -> Result<(), LinkError> {
        self.action("syncData", |s| s.1.pages = s.0.pages.clone())
    }
    fn write_metadata(&mut self, slot: usize, bytes: &[u8; 128]) -> Result<(), LinkError> {
        self.action(&format!("writeMetadata{slot}"), |s| {
            s.0.metadata[slot] = Some(bytes.to_vec())
        })
    }
    fn sync_metadata(&mut self, slot: usize) -> Result<(), LinkError> {
        self.action(&format!("syncMetadata{slot}"), |s| {
            s.1.metadata[slot] = s.0.metadata[slot].clone()
        })
    }
    fn truncate_data(&mut self, length: u64) -> Result<(), LinkError> {
        self.action("truncateData", |s| {
            s.0.pages.truncate(length as usize / 4096)
        })
    }
}
impl RepairPageDevice for Device {
    fn repair_page(&mut self, index: u64, bytes: &[u8; 4096]) -> Result<(), LinkError> {
        self.action("repairPage", |s| {
            assert!(index as usize <= s.0.pages.len());
            if index as usize == s.0.pages.len() {
                s.0.pages.push(*bytes);
            } else {
                s.0.pages[index as usize] = *bytes;
            }
        })
    }
}
pub struct Pair {
    pub devices: [Device; 2],
    pub trace: Rc<RefCell<Trace>>,
}
impl Pair {
    pub fn new(images: [Image; 2]) -> Self {
        let trace = Rc::new(RefCell::new(Trace::default()));
        let devices = std::array::from_fn(|side| Device {
            state: Rc::new(RefCell::new((images[side].clone(), images[side].clone()))),
            trace: trace.clone(),
            side,
            faults: Rc::new(RefCell::new(ReadFaults::default())),
        });
        Self { devices, trace }
    }
    pub fn open(&self) -> Result<PagedLog<MirroredPageDevice<Device>>, LinkError> {
        PagedLog::open(MirroredPageDevice::new(
            self.devices[0].clone(),
            self.devices[1].clone(),
        ))
    }
    pub fn crash(&self) -> Self {
        Self::new(self.devices.each_ref().map(Device::durable))
    }
    pub fn clear(&self) {
        self.trace.borrow_mut().calls.clear();
        self.trace.borrow_mut().reads.clear();
    }
}
pub fn wire() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../docs/protocol/native-pages-mirrored-v1.json"
    ))
    .unwrap()
}
pub fn hex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
        .collect()
}
pub fn fixture(name: &str) -> (Image, Vec<String>) {
    let v = wire();
    let v = &v[name];
    (
        Image {
            metadata: std::array::from_fn(|i| {
                let t = v["metadata"][i].as_str().unwrap();
                if t.is_empty() {
                    None
                } else {
                    Some(hex(t))
                }
            }),
            pages: v["pages"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| hex(v.as_str().unwrap()).try_into().unwrap())
                .collect(),
        },
        v["records"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().into())
            .collect(),
    )
}
pub fn hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(14695981039346656037, |s, b| {
        (s ^ u64::from(*b)).wrapping_mul(1099511628211)
    })
}
pub fn sign_page(page: &mut [u8; 4096]) -> u64 {
    let bytes = [&page[..56], &page[64..]].concat();
    let h = hash(&bytes);
    page[56..64].copy_from_slice(&h.to_le_bytes());
    h
}
pub fn sign_meta(meta: &mut [u8]) {
    let h = hash(&meta[..120]);
    meta[120..].copy_from_slice(&h.to_le_bytes());
}
pub fn following(pair: &Pair, expected: &[String]) -> Result<(), LinkError> {
    let mut log = pair.open()?;
    assert_eq!(log.read_log_entries()?, expected);
    log.append_log_entry("following")?;
    log.flush_log()?;
    assert_eq!(
        pair.crash().open()?.read_log_entries()?,
        [expected.to_vec(), vec!["following".into()]].concat()
    );
    Ok(())
}
