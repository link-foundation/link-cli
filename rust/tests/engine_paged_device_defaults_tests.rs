//! What a page device gets from the trait when it declares nothing, and the
//! one device mode a log refuses to interpret. Runs unchanged in actual WASM.
use link_cli::engine::paged::{PageDevice, PagedLog, PAGE_SIZE};
use link_cli::{LinkError, TransitionLogStore};

/// An in-memory device implementing only what the trait requires.
#[derive(Default)]
struct Bare {
    flags: u32,
    metadata: [Option<Vec<u8>>; 2],
    pages: Vec<[u8; PAGE_SIZE]>,
}

impl PageDevice for Bare {
    fn metadata_flags(&self) -> u32 {
        self.flags
    }
    fn new_database_id(&mut self) -> Result<[u8; 16], LinkError> {
        Ok([7; 16])
    }
    fn data_length(&mut self) -> Result<u64, LinkError> {
        Ok((self.pages.len() * PAGE_SIZE) as u64)
    }
    fn read_metadata(&mut self, slot: usize) -> Result<Option<Vec<u8>>, LinkError> {
        Ok(self.metadata[slot].clone())
    }
    fn write_metadata(&mut self, slot: usize, bytes: &[u8; 128]) -> Result<(), LinkError> {
        self.metadata[slot] = Some(bytes.to_vec());
        Ok(())
    }
    fn sync_metadata(&mut self, _: usize) -> Result<(), LinkError> {
        Ok(())
    }
    fn read_page(&mut self, index: u64) -> Result<[u8; PAGE_SIZE], LinkError> {
        Ok(self.pages[index as usize])
    }
    fn write_page(&mut self, index: u64, bytes: &[u8; PAGE_SIZE]) -> Result<(), LinkError> {
        if index as usize == self.pages.len() {
            self.pages.push(*bytes);
        } else {
            self.pages[index as usize] = *bytes;
        }
        Ok(())
    }
    fn sync_data(&mut self) -> Result<(), LinkError> {
        Ok(())
    }
    fn truncate_data(&mut self, length: u64) -> Result<(), LinkError> {
        self.pages.truncate(length as usize / PAGE_SIZE);
        Ok(())
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn a_device_that_promises_nothing_works_but_is_not_durable() -> Result<(), LinkError> {
    let mut log = PagedLog::open(Bare::default())?;
    assert!(!log.supports_durable_flush());
    log.append_log_entry("one")?;
    log.flush_log()?;
    assert_eq!(log.read_log_entries()?, ["one"]);
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn a_device_in_an_unknown_mode_is_refused() {
    let device = Bare {
        flags: 2,
        ..Bare::default()
    };
    let refusal = PagedLog::open(device).err().unwrap().to_string();
    assert!(refusal.contains("unsupported device mode"), "{refusal}");
}
