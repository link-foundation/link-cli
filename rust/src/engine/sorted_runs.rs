//! Optional immutable sorted runs. Lower tiers always contain newer edits.
use std::{collections::BTreeMap, sync::Arc, sync::OnceLock};
#[cfg(test)]
#[path = "sorted_runs_tests.rs"]
mod tests;

type Run<K, V> = Arc<Vec<(K, Option<V>)>>;

#[derive(Debug)]
pub(super) struct SortedRuns<K, V> {
    levels: Vec<Option<Run<K, V>>>,
    len: usize,
    ordered: OnceLock<BTreeMap<K, V>>,
}

impl<K, V> Clone for SortedRuns<K, V> {
    fn clone(&self) -> Self {
        Self {
            levels: self.levels.clone(),
            len: self.len,
            ordered: OnceLock::new(),
        }
    }
}

impl<K: Ord + Clone, V: Clone + PartialEq> SortedRuns<K, V> {
    pub(super) fn new() -> Self {
        Self {
            levels: Vec::new(),
            len: 0,
            ordered: OnceLock::new(),
        }
    }

    pub(super) fn get(&self, key: &K) -> Option<&V> {
        for run in self.levels.iter().flatten() {
            if let Ok(index) = run.binary_search_by(|(id, _)| id.cmp(key)) {
                // A found tombstone terminates lookup; older values stay hidden.
                return run[index].1.as_ref();
            }
        }
        None
    }

    pub(super) fn set(&mut self, key: K, value: Option<V>) {
        let present = self.get(&key);
        if present == value.as_ref() {
            return;
        }
        match (present.is_some(), value.is_some()) {
            (false, true) => self.len += 1,
            (true, false) => self.len -= 1,
            _ => {}
        }
        self.ordered.take();
        let mut next = vec![(key, value)];
        let mut tier = 0;
        loop {
            if tier == self.levels.len() {
                self.levels.push(None);
            }
            if let Some(older) = self.levels[tier].take() {
                next = Self::merge(&older, next);
                tier += 1;
                continue;
            }
            // No older run remains: dropping tombstones cannot resurrect data.
            // Old snapshots own separate immutable arrays and remain unaffected.
            if self.levels[tier + 1..].iter().all(Option::is_none) {
                next.retain(|(_, value)| value.is_some());
            }
            if !next.is_empty() {
                self.levels[tier] = Some(Arc::new(next));
            }
            while self.levels.last().is_some_and(Option::is_none) {
                self.levels.pop();
            }
            break;
        }
    }

    fn merge(older: &[(K, Option<V>)], newer: Vec<(K, Option<V>)>) -> Vec<(K, Option<V>)> {
        let mut merged = Vec::with_capacity(older.len() + newer.len());
        let mut old = older.iter().peekable();
        let mut new = newer.into_iter().peekable();
        while let (Some(a), Some(b)) = (old.peek(), new.peek()) {
            match a.0.cmp(&b.0) {
                std::cmp::Ordering::Less => merged.push(old.next().unwrap().clone()),
                std::cmp::Ordering::Equal => {
                    old.next();
                    merged.push(new.next().unwrap());
                }
                std::cmp::Ordering::Greater => merged.push(new.next().unwrap()),
            }
        }
        merged.extend(old.cloned());
        merged.extend(new);
        merged
    }

    pub(super) fn ordered(&self) -> &BTreeMap<K, V> {
        self.ordered.get_or_init(|| {
            let mut rows = BTreeMap::new();
            for run in self.levels.iter().rev().flatten() {
                for (key, value) in run.iter() {
                    if let Some(value) = value {
                        rows.insert(key.clone(), value.clone());
                    } else {
                        rows.remove(key);
                    }
                }
            }
            rows
        })
    }

    pub(super) fn into_ordered(self) -> BTreeMap<K, V> {
        self.ordered();
        self.ordered.into_inner().expect("initialized ordered view")
    }
}
