//! Selected immutable map; flat engines retain their existing allocation path.
use super::sorted_runs::SortedRuns;
use super::{Row, StateStructure};
use rpds::RedBlackTreeMapSync;
use std::{collections::BTreeMap, sync::OnceLock};

#[derive(Debug)]
pub(super) enum Rows<T> {
    Flat(BTreeMap<T, Row<T>>),
    SortedRuns(SortedRuns<T, Row<T>>),
    Shared {
        map: RedBlackTreeMapSync<T, Row<T>>,
        // Only the legacy public BTreeMap accessor materializes this copy.
        compatibility: OnceLock<BTreeMap<T, Row<T>>>,
    },
}

impl<T: Ord + Clone> Clone for Rows<T> {
    fn clone(&self) -> Self {
        match self {
            Self::Flat(map) => Self::Flat(map.clone()),
            Self::SortedRuns(map) => Self::SortedRuns(map.clone()),
            Self::Shared { map, .. } => Self::Shared {
                map: map.clone(),
                compatibility: OnceLock::new(),
            },
        }
    }
}

impl<T: Ord + Clone> Rows<T> {
    pub(super) fn new(structure: StateStructure) -> Self {
        match structure {
            StateStructure::Flat => Self::Flat(BTreeMap::new()),
            StateStructure::SortedRuns => Self::SortedRuns(SortedRuns::new()),
            StateStructure::Shared => Self::Shared {
                map: RedBlackTreeMapSync::new_sync(),
                compatibility: OnceLock::new(),
            },
        }
    }
    pub(super) fn get(&self, id: &T) -> Option<&Row<T>> {
        match self {
            Self::Flat(map) => map.get(id),
            Self::SortedRuns(map) => map.get(id),
            Self::Shared { map, .. } => map.get(id),
        }
    }
    pub(super) fn contains_key(&self, id: &T) -> bool {
        self.get(id).is_some()
    }
    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.iter().next().is_none()
    }
    pub(super) fn iter(&self) -> impl Iterator<Item = (&T, &Row<T>)> {
        // Inferred iterator types keep the flat path free of boxed iterators.
        let (mut flat, mut shared) = match self {
            Self::Flat(map) => (Some(map.iter()), None),
            Self::SortedRuns(map) => (Some(map.ordered().iter()), None),
            Self::Shared { map, .. } => (None, Some(map.iter())),
        };
        std::iter::from_fn(move || match &mut flat {
            Some(iterator) => iterator.next(),
            None => shared.as_mut().expect("selected shared iterator").next(),
        })
    }
    pub(super) fn keys(&self) -> impl Iterator<Item = &T> {
        self.iter().map(|(id, _)| id)
    }
    pub(super) fn values(&self) -> impl Iterator<Item = &Row<T>> {
        self.iter().map(|(_, row)| row)
    }
    pub(super) fn insert(&mut self, id: T, row: Row<T>) {
        match self {
            Self::Flat(map) => {
                map.insert(id, row);
            }
            Self::SortedRuns(map) => map.set(id, Some(row)),
            Self::Shared { map, compatibility } => {
                compatibility.take();
                map.insert_mut(id, row);
            }
        }
    }
    pub(super) fn remove(&mut self, id: &T) {
        match self {
            Self::Flat(map) => {
                map.remove(id);
            }
            Self::SortedRuns(map) => map.set(id.clone(), None),
            Self::Shared { map, compatibility } => {
                compatibility.take();
                map.remove_mut(id);
            }
        }
    }
    pub(super) fn materialize(&self) -> &BTreeMap<T, Row<T>> {
        match self {
            Self::Flat(map) => map,
            Self::SortedRuns(map) => map.ordered(),
            Self::Shared { compatibility, .. } => compatibility.get_or_init(|| {
                self.iter()
                    .map(|(id, row)| (id.clone(), row.clone()))
                    .collect()
            }),
        }
    }
    /// Convert foreign/freshly decoded rows to the receiving engine's choice.
    /// Recovery reuses unchanged entries from the preceding immutable revision.
    pub(super) fn adapt(self, structure: StateStructure, previous: Option<&Self>) -> Self
    where
        T: PartialEq,
    {
        let incoming = match (self, structure) {
            (rows @ Self::Flat(_), StateStructure::Flat)
            | (rows @ Self::Shared { .. }, StateStructure::Shared)
            | (rows @ Self::SortedRuns(_), StateStructure::SortedRuns) => return rows,
            (Self::Flat(map), _) => map,
            (Self::SortedRuns(map), _) => map.into_ordered(),
            (Self::Shared { map, compatibility }, _) => {
                compatibility.into_inner().unwrap_or_else(|| {
                    map.iter()
                        .map(|(id, row)| (id.clone(), row.clone()))
                        .collect()
                })
            }
        };
        if structure == StateStructure::Flat {
            return Self::Flat(incoming);
        }
        let mut rows = match previous {
            Some(rows @ Self::Shared { .. }) if structure == StateStructure::Shared => rows.clone(),
            Some(rows @ Self::SortedRuns(_)) if structure == StateStructure::SortedRuns => {
                rows.clone()
            }
            _ => Self::new(structure),
        };
        let removed: Vec<_> = rows
            .keys()
            .filter(|id| !incoming.contains_key(*id))
            .cloned()
            .collect();
        for id in removed {
            rows.remove(&id);
        }
        for (id, row) in incoming {
            if rows.get(&id) != Some(&row) {
                rows.insert(id, row);
            }
        }
        rows
    }
}

impl<T: Ord + Clone + PartialEq> PartialEq for Rows<T> {
    fn eq(&self, other: &Self) -> bool {
        self.iter().eq(other.iter())
    }
}
