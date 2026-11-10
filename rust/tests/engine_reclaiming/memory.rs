use super::*;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};
type Hook = Box<dyn FnOnce() -> Result<(), LinkError> + Send>;
#[derive(Clone, Default)]
pub(super) struct Store(pub Arc<Mutex<State>>);
#[derive(Default)]
pub(super) struct State {
    pub values: BTreeMap<String, JournalVersion>,
    pub revision: u64,
    pub calls: Vec<(&'static str, Vec<String>)>,
    pub durable: bool,
    pub read_override: Option<Vec<JournalVersion>>,
    pub scan_override: Option<Vec<JournalNamespaceEntry>>,
    hooks: BTreeMap<(&'static str, bool), VecDeque<Hook>>,
}
impl Store {
    pub fn on(
        &self,
        operation: &'static str,
        after: bool,
        hook: impl FnOnce() -> Result<(), LinkError> + Send + 'static,
    ) {
        self.0
            .lock()
            .unwrap()
            .hooks
            .entry((operation, after))
            .or_default()
            .push_back(Box::new(hook));
    }
    pub fn count(&self, operation: &str) -> usize {
        self.0
            .lock()
            .unwrap()
            .calls
            .iter()
            .filter(|(op, _)| *op == operation)
            .count()
    }
    fn hook(&self, operation: &'static str, after: bool) -> Result<(), LinkError> {
        let hook = self
            .0
            .lock()
            .unwrap()
            .hooks
            .get_mut(&(operation, after))
            .and_then(VecDeque::pop_front);
        if let Some(hook) = hook {
            hook()?;
        }
        Ok(())
    }
    fn run<R>(
        &self,
        op: &'static str,
        keys: &[String],
        action: impl FnOnce(&mut State) -> Result<R, LinkError>,
    ) -> Result<R, LinkError> {
        self.0.lock().unwrap().calls.push((op, keys.to_vec()));
        self.hook(op, false)?;
        let result = action(&mut self.0.lock().unwrap())?;
        self.hook(op, true)?;
        Ok(result)
    }
}
impl State {
    fn next(&mut self) -> Result<u64, LinkError> {
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or_else(|| error("fake generation exhausted"))?;
        Ok(self.revision)
    }
}
impl AtomicJournalStore for Store {
    fn supports_durable_flush(&self) -> bool {
        self.0.lock().unwrap().durable
    }
    fn read(&mut self, keys: &[String]) -> Result<Vec<JournalVersion>, LinkError> {
        self.run("read", keys, |state| {
            if let Some(result) = state.read_override.take() {
                return Ok(result);
            }
            Ok(keys
                .iter()
                .map(|key| state.values.get(key).cloned().unwrap_or_default())
                .collect())
        })
    }
    fn compare_exchange(
        &mut self,
        keys: &[String],
        expected: &[JournalVersion],
        updates: &[Option<Vec<String>>],
    ) -> Result<bool, LinkError> {
        assert_eq!(keys.len(), expected.len());
        assert_eq!(keys.len(), updates.len());
        self.run("swap", keys, |state| {
            if keys.iter().zip(expected).any(|(key, item)| {
                state.values.get(key).map_or(0, |v| v.generation) != item.generation
            }) {
                return Ok(false);
            }
            if updates.iter().all(Option::is_none) {
                return Ok(true);
            }
            let generation = state.next()?;
            for (key, update) in keys.iter().zip(updates) {
                if let Some(entries) = update {
                    state.values.insert(
                        key.clone(),
                        JournalVersion {
                            generation,
                            entries: entries.clone(),
                        },
                    );
                }
            }
            Ok(true)
        })
    }
}
impl JournalMaintenanceStore for Store {
    fn scan_namespace(
        &mut self,
        options: JournalScanOptions,
    ) -> Result<Vec<JournalNamespaceEntry>, LinkError> {
        self.run("scan", &[], |state| {
            if let Some(result) = state.scan_override.take() {
                return Ok(result);
            }
            let mut bytes = 0;
            let result: Vec<_> = state
                .values
                .iter()
                .map(|(key, value)| {
                    let mut value = value.clone();
                    if options.keys_only {
                        value.entries.clear();
                    }
                    bytes += key.len()
                        + 8
                        + value
                            .entries
                            .iter()
                            .map(|record| record.len() + 1)
                            .sum::<usize>();
                    JournalNamespaceEntry {
                        key: key.clone(),
                        version: value,
                    }
                })
                .collect();
            if result.len() > options.max_keys || bytes > options.max_bytes {
                return Err(error("complete fake scan limit"));
            }
            Ok(result)
        })
    }
    fn compare_delete(&mut self, key: &str, expected: u64) -> Result<bool, LinkError> {
        assert_ne!(expected, 0);
        self.run("delete", &[key.into()], |state| {
            if state.values.get(key).map(|v| v.generation) != Some(expected) {
                return Ok(false);
            }
            state.next()?;
            state.values.remove(key);
            Ok(true)
        })
    }
}
