//! Optional immutable constraint indexes. Candidates never mutate committed roots.
use super::{error, Constraint, Row, Rows};
use crate::LinkError;
use doublets::data::LinkReference;
use rpds::RedBlackTreeMapSync;

pub(super) struct ConstraintIndex<T> {
    pub(super) pairs: Option<RedBlackTreeMapSync<(T, T), T>>,
    pub(super) names: Option<RedBlackTreeMapSync<String, T>>,
    pub(super) references: Option<RedBlackTreeMapSync<T, usize>>,
}

impl<T: Ord + Clone> Clone for ConstraintIndex<T> {
    fn clone(&self) -> Self {
        Self {
            pairs: self.pairs.clone(),
            names: self.names.clone(),
            references: self.references.clone(),
        }
    }
}

impl<T: LinkReference> ConstraintIndex<T> {
    pub(super) fn build(
        constraints: &[Constraint],
        rows: &Rows<T>,
    ) -> Result<Option<Self>, LinkError> {
        if constraints.is_empty() {
            return Ok(None);
        }
        let index = Self {
            pairs: constraints
                .contains(&Constraint::UniquePairs)
                .then(RedBlackTreeMapSync::new_sync),
            names: constraints
                .contains(&Constraint::UniqueNames)
                .then(RedBlackTreeMapSync::new_sync),
            references: constraints
                .contains(&Constraint::References)
                .then(RedBlackTreeMapSync::new_sync),
        };
        index
            .prepare(
                &Rows::new(super::StateStructure::Flat),
                rows,
                &rows.keys().copied().collect::<Vec<_>>(),
            )
            .map(Some)
    }

    pub(super) fn prepare(
        &self,
        before: &Rows<T>,
        after: &Rows<T>,
        changed: &[T],
    ) -> Result<Self, LinkError> {
        let mut candidate = self.clone();
        // Remove every old key first: swaps and transfers are atomic in the final
        // candidate even if an intermediate key would otherwise appear occupied.
        for id in changed {
            let old = before.get(id);
            let new = after.get(id);
            if let Some(old) = old {
                if Some(pair(old)) != new.map(pair) {
                    if let Some(pairs) = &mut candidate.pairs {
                        pairs.remove_mut(&pair(old));
                    }
                    candidate.change_references(old, false);
                }
                if old.name.as_ref() != new.and_then(|row| row.name.as_ref()) {
                    if let (Some(names), Some(name)) = (&mut candidate.names, &old.name) {
                        names.remove_mut(name);
                    }
                }
            }
        }
        for id in changed {
            let old = before.get(id);
            let new = after.get(id);
            if let Some(new) = new {
                if Some(pair(new)) != old.map(pair) {
                    if let Some(pairs) = &mut candidate.pairs {
                        if pairs.get(&pair(new)).is_some_and(|owner| owner != id) {
                            return Err(error("constraint: duplicate pair"));
                        }
                        pairs.insert_mut(pair(new), *id);
                    }
                    candidate.change_references(new, true);
                }
                if new.name.as_ref() != old.and_then(|row| row.name.as_ref()) {
                    if let (Some(names), Some(name)) = (&mut candidate.names, &new.name) {
                        if names.get(name).is_some_and(|owner| owner != id) {
                            return Err(error("constraint: duplicate name"));
                        }
                        names.insert_mut(name.clone(), *id);
                    }
                }
            }
        }
        if let Some(references) = &candidate.references {
            let zero = T::from_byte(0);
            for id in changed {
                match after.get(id) {
                    Some(row) => {
                        if [row.link.source, row.link.target]
                            .into_iter()
                            .any(|target| target != zero && !after.contains_key(&target))
                        {
                            return Err(error("constraint: dangling link reference"));
                        }
                    }
                    None if references.get(id).is_some() => {
                        return Err(error("constraint: dangling link reference"))
                    }
                    None => {}
                }
            }
        }
        Ok(candidate)
    }

    fn change_references(&mut self, row: &Row<T>, add: bool) {
        if let Some(references) = &mut self.references {
            for id in [row.link.source, row.link.target] {
                if id == T::from_byte(0) {
                    continue;
                }
                let count = references.get(&id).copied().unwrap_or_default();
                if add {
                    references.insert_mut(id, count + 1);
                } else if count > 1 {
                    references.insert_mut(id, count - 1);
                } else {
                    references.remove_mut(&id);
                }
            }
        }
    }
}

fn pair<T: Copy>(row: &Row<T>) -> (T, T) {
    (row.link.source, row.link.target)
}
