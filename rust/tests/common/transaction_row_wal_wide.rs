//! Sparse u64 replay; no dense allocator ever sees a 2^32-address request.
use super::*;
use link_cli::GenericLink;

#[derive(Clone, Default)]
struct WideImage {
    rows: BTreeMap<u64, GenericLink<u64>>,
    names: BTreeMap<u64, String>,
}
struct Sparse {
    image: WideImage,
    durable: Rc<RefCell<WideImage>>,
    next: u64,
    named: bool,
}
impl LinksStorage<u64> for Sparse {
    fn create_link(&mut self, source: u64, target: u64) -> Result<u64, LinkError> {
        let id = self.next;
        self.next += 1;
        self.image
            .rows
            .insert(id, GenericLink::new(id, source, target));
        Ok(id)
    }
    fn ensure_link_created(&mut self, index: u64) -> Result<u64, LinkError> {
        self.image
            .rows
            .entry(index)
            .or_insert(GenericLink::new(index, 0, 0));
        self.next = self.next.max(index + 1);
        Ok(index)
    }
    fn update_link(
        &mut self,
        index: u64,
        source: u64,
        target: u64,
    ) -> Result<GenericLink<u64>, LinkError> {
        let before = self
            .image
            .rows
            .get_mut(&index)
            .ok_or(LinkError::not_found(index))?;
        Ok(std::mem::replace(
            before,
            GenericLink::new(index, source, target),
        ))
    }
    fn delete_link(&mut self, index: u64) -> Result<GenericLink<u64>, LinkError> {
        self.image
            .rows
            .remove(&index)
            .ok_or(LinkError::not_found(index))
    }
    fn get_link(&self, index: u64) -> Option<GenericLink<u64>> {
        self.image.rows.get(&index).copied()
    }
    fn all_links(&self) -> Vec<GenericLink<u64>> {
        self.image.rows.values().copied().collect()
    }
    fn capture_names(&mut self) -> Result<Option<BTreeMap<u64, String>>, LinkError> {
        Ok(self.named.then(|| self.image.names.clone()))
    }
    fn set_link_name(&mut self, index: u64, name: Option<&str>) -> Result<u64, LinkError> {
        if let Some(name) = name {
            self.image.names.insert(index, name.into());
        } else {
            self.image.names.remove(&index);
        }
        Ok(index)
    }
    fn flush(&mut self) -> Result<(), LinkError> {
        *self.durable.borrow_mut() = self.image.clone();
        Ok(())
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn wide_images_recover_and_rollback_through_sparse_adapters() {
    const HIGH: u64 = 1 << 32;
    let vectors: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/protocol/transaction-undo-vectors.json"
    ))
    .unwrap();
    for named in [false, true] {
        let wire = vectors["wide"].as_str().unwrap();
        let wire = if named {
            wire.into()
        } else {
            format!("{}|-", wire.rsplit_once('|').unwrap().0)
        };
        let state: Shared = Rc::default();
        state.borrow_mut().image.journal = vec![format!("__transactions:transition:{wire}")];
        let durable = Rc::new(RefCell::new(WideImage::default()));
        for reopen in 0..2 {
            let image = durable.borrow().clone();
            let entries = state.borrow().image.journal.clone();
            let mut tx = GenericTransactionsDecorator::new(
                Sparse {
                    image,
                    durable: durable.clone(),
                    next: HIGH + 1,
                    named,
                },
                Log {
                    entries,
                    control: state.clone(),
                },
                LogRetentionPolicy::Infinite,
                CommitMode::Sync,
                true,
            )
            .unwrap()
            .with_row_undo(link_cli::RowUndoPolicy::WriteAheadImage)
            .unwrap();
            assert_eq!(
                tx.inner().all_links(),
                vec![GenericLink::new(HIGH, HIGH + 1, 0)]
            );
            assert_eq!(
                tx.inner_mut().capture_names().unwrap(),
                named.then(|| BTreeMap::from([(0, String::new())]))
            );
            tx.begin_transaction().unwrap();
            tx.update(HIGH, HIGH + 2, HIGH).unwrap();
            if named {
                tx.set_name(HIGH, "wide 雪").unwrap();
            }
            tx.rollback().unwrap();
            assert_eq!(
                tx.inner().get_link(HIGH),
                Some(GenericLink::new(HIGH, HIGH + 1, 0))
            );
            assert_eq!(
                tx.inner_mut().capture_names().unwrap(),
                named.then(|| BTreeMap::from([(0, String::new())]))
            );
            if reopen == 1 {
                let id = tx.create(HIGH, HIGH + 1).unwrap();
                assert_eq!(id, HIGH + 1);
                assert_eq!(tx.inner().all_links().len(), 2);
            }
        }
    }
}
