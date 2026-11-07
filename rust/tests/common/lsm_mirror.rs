use super::{
    harness::{Device, Harness},
    options, put,
};
use link_cli::{
    engine::{
        cluster::AtomicJournalStore,
        lsm::{LsmTableNamespace, LsmWalState, PhysicalLsmJournalStore},
        paged::{MirroredPageDevice, PageDevice, RepairPageDevice},
    },
    LinkError, ReplaceableTransitionLogStore, TransitionLogStore,
};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};
#[derive(Clone, Default)]
struct Namespace(Rc<RefCell<BTreeMap<u64, [Device; 2]>>>);
impl LsmTableNamespace for Namespace {
    type Device = MirroredPageDevice<Device>;
    fn supports_durable_flush(&self) -> bool {
        true
    }
    fn list(&mut self, max: usize) -> Result<Vec<u64>, LinkError> {
        assert!(self.0.borrow().len() <= max);
        Ok(self.0.borrow().keys().copied().collect())
    }
    fn create(&mut self, id: u64) -> Result<Self::Device, LinkError> {
        assert!(self
            .0
            .borrow_mut()
            .insert(id, [Device::default(), Device::default()])
            .is_none());
        self.open(id)
    }
    fn open(&mut self, id: u64) -> Result<Self::Device, LinkError> {
        let pair = self.0.borrow().get(&id).unwrap().clone();
        Ok(MirroredPageDevice::new(pair[0].clone(), pair[1].clone()))
    }
    fn close(&mut self, _: &mut Self::Device) -> Result<(), LinkError> {
        Ok(())
    }
    fn delete(&mut self, id: u64) -> Result<(), LinkError> {
        assert!(self.0.borrow_mut().remove(&id).is_some());
        Ok(())
    }
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn physical_store_composes_independent_mirrored_tables_and_repairs_surviving_copy(
) -> Result<(), LinkError> {
    let h = Harness::new();
    let p = Namespace::default();
    let mut s = PhysicalLsmJournalStore::open(h.wal()?, p.clone(), options())?;
    put(&mut s, "a", vec!["雪".into()])?;
    s.flush()?;
    assert_eq!(s.read(&["a".into()])?[0].entries, ["雪"]);
    s.close()?;
    let mut pair = p.0.borrow().get(&1).unwrap().clone();
    pair[0].repair_page(0, &[0; 4096])?;
    pair[0].sync_data()?;
    let mut fresh = PhysicalLsmJournalStore::open(h.wal()?, p, options())?;
    assert_eq!(fresh.read(&["a".into()])?[0].entries, ["雪"]);
    assert_eq!(pair[0].read_page(0)?, pair[1].read_page(0)?);
    fresh.close()?;
    Ok(())
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn later_bad_binding_preserves_staged_mirrored_repairs_in_earlier_table() -> Result<(), LinkError> {
    let h = Harness::new();
    let p = Namespace::default();
    let mut s = PhysicalLsmJournalStore::open(h.wal()?, p.clone(), options())?;
    for key in ["a", "b"] {
        put(&mut s, key, vec![key.into()])?;
        s.flush()?;
    }
    s.close()?;
    let mut first = p.0.borrow().get(&1).unwrap()[0].clone();
    first.repair_page(0, &[0; 4096])?;
    first.sync_data()?;
    let mut wal = h.wal()?;
    let mut state = LsmWalState::recover(&wal.read_log_entries()?, options().limits)?;
    state.manifest[1].digest ^= 1;
    wal.replace_log_entries(&[state.encode(options().limits)?])?;
    let error = PhysicalLsmJournalStore::open(wal, p, options())
        .err()
        .unwrap();
    assert!(error.to_string().contains("binding changed"));
    assert_eq!(first.read_page(0)?, [0; 4096]);
    Ok(())
}
