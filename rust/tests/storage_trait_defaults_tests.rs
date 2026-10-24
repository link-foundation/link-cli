//! Raw default algorithms also serve external stores that implement only CRUD.
use link_cli::{GenericLink, LinkError, LinksStorage, LinksStorageRef, StorageRevision};
use std::collections::BTreeMap;

#[derive(Default)]
struct TinyStore {
    rows: BTreeMap<u64, GenericLink<u64>>,
    next: u64,
    fail_create: bool,
    hide_after_update: bool,
}
impl LinksStorage<u64> for TinyStore {
    fn create_link(&mut self, source: u64, target: u64) -> Result<u64, LinkError> {
        if self.fail_create {
            return Err(LinkError::Transaction("injected create failure".into()));
        }
        self.next += 1;
        self.rows
            .insert(self.next, GenericLink::new(self.next, source, target));
        Ok(self.next)
    }
    fn ensure_link_created(&mut self, id: u64) -> Result<u64, LinkError> {
        self.next = self.next.max(id);
        self.rows.entry(id).or_insert(GenericLink::new(id, 0, 0));
        Ok(id)
    }
    fn get_link(&self, id: u64) -> Option<GenericLink<u64>> {
        self.rows.get(&id).copied()
    }
    fn update_link(
        &mut self,
        id: u64,
        source: u64,
        target: u64,
    ) -> Result<GenericLink<u64>, LinkError> {
        let before = self
            .rows
            .get(&id)
            .copied()
            .ok_or(LinkError::not_found(id))?;
        if self.hide_after_update {
            self.rows.remove(&id);
        } else {
            self.rows.insert(id, GenericLink::new(id, source, target));
        }
        Ok(before)
    }
    fn delete_link(&mut self, id: u64) -> Result<GenericLink<u64>, LinkError> {
        self.rows.remove(&id).ok_or(LinkError::not_found(id))
    }
    fn all_links(&self) -> Vec<GenericLink<u64>> {
        self.rows.values().copied().collect()
    }
    fn flush(&mut self) -> Result<(), LinkError> {
        Ok(())
    }
}
impl LinksStorageRef<u64> for TinyStore {
    fn get_link_ref(&self, id: u64) -> Option<&GenericLink<u64>> {
        self.rows.get(&id)
    }
    fn all_link_refs(&self) -> Vec<&GenericLink<u64>> {
        self.rows.values().collect()
    }
}

#[test]
fn raw_and_borrowed_default_queries_apply_every_optional_filter() {
    let mut store = TinyStore::default();
    store.create_link(2, 3).unwrap();
    store.create_link(3, 2).unwrap();
    store.create_link(2, 2).unwrap();
    let cases = [
        ((None, None, None), vec![1, 2, 3]),
        ((Some(1), None, None), vec![1]),
        ((None, Some(2), None), vec![1, 3]),
        ((None, None, Some(2)), vec![2, 3]),
        ((Some(1), Some(2), None), vec![1]),
        ((Some(1), None, Some(3)), vec![1]),
        ((None, Some(2), Some(2)), vec![3]),
        ((Some(1), Some(2), Some(3)), vec![1]),
        ((Some(99), None, None), vec![]),
        ((Some(1), Some(99), None), vec![]),
        ((Some(1), Some(2), Some(99)), vec![]),
    ];
    for ((id, source, target), expected) in cases {
        assert_eq!(
            store
                .query_links(id, source, target)
                .iter()
                .map(|l| l.index)
                .collect::<Vec<_>>(),
            expected
        );
        assert_eq!(
            store
                .query_link_refs(id, source, target)
                .iter()
                .map(|l| l.index)
                .collect::<Vec<_>>(),
            expected
        );
    }
    assert_eq!(store.links_count(), 3);
    assert!(store.link_exists(1));
    assert!(!store.link_exists(99));
    assert_eq!(store.get_link_ref(1).unwrap().target, 3);
    assert!(store.get_link_ref(99).is_none());
    assert!(!store.has_external_changes().unwrap());
    store.reload().unwrap();
    store.flush().unwrap();
    assert_eq!(store.links_count(), 3);
}

#[test]
fn default_search_and_get_or_create_preserve_addresses_and_errors() {
    let mut store = TinyStore::default();
    store.create_link(2, 3).unwrap();
    store.create_link(3, 2).unwrap();
    assert_eq!(store.search_link(3, 2), Some(2));
    assert_eq!(store.search_link(2, 2), None);
    assert_eq!(store.get_or_create_link(2, 3).unwrap(), 1);
    assert_eq!(store.get_or_create_link(2, 2).unwrap(), 3);
    assert_eq!(store.links_count(), 3);
    store.fail_create = true;
    assert!(store
        .get_or_create_link(9, 9)
        .unwrap_err()
        .to_string()
        .contains("injected create failure"));
    assert_eq!(store.links_count(), 3);
}

#[test]
fn default_observers_run_once_after_success_and_never_after_a_failed_write() {
    for hide_after_update in [false, true] {
        let mut store = TinyStore {
            hide_after_update,
            ..Default::default()
        };
        let id = store.create_link(2, 3).unwrap();
        let mut events = Vec::new();
        let before = store
            .update_link_observed(id, 3, 2, &mut |a, b| events.push((a, b)))
            .unwrap();
        assert_eq!(before, GenericLink::new(1, 2, 3));
        assert_eq!(events, [(before, GenericLink::new(1, 3, 2))]);
        assert!(store
            .update_link_observed(99, 0, 0, &mut |a, b| events.push((a, b)))
            .is_err());
        assert_eq!(events.len(), 1);
        store.ensure_link_created(4).unwrap();
        assert_eq!(
            store
                .delete_link_observed(4, &mut |a, b| events.push((a, b)))
                .unwrap(),
            GenericLink::new(4, 0, 0)
        );
        assert_eq!(events[1], (GenericLink::new(4, 0, 0), GenericLink::null()));
        assert!(store
            .delete_link_observed(99, &mut |a, b| events.push((a, b)))
            .is_err());
        assert_eq!(events.len(), 2);
    }
}

#[test]
fn filesystem_revision_distinguishes_missing_present_changed_and_invalid_paths() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data");
    assert_eq!(
        StorageRevision::of(&path).unwrap(),
        StorageRevision::default()
    );
    std::fs::write(&path, b"first").unwrap();
    let before = StorageRevision::of(&path).unwrap();
    assert_eq!(before.len, 5);
    std::fs::write(&path, b"longer value").unwrap();
    let after = StorageRevision::of(&path).unwrap();
    assert_eq!(after.len, 12);
    assert_ne!(before, after);
    assert!(matches!(
        StorageRevision::of("invalid\0path"),
        Err(LinkError::Io(_))
    ));
}
