//! Selected owned projection. Visibility never discards causal evidence.
use super::{
    causal::{CausalDot, CausalVersion},
    Row,
};
use crate::{GenericLink, LinkError};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CausalConflictPolicy {
    Reject,
    MaxDot,
}

#[derive(Clone, Debug)]
pub struct CausalProjectionOptions {
    pub max_versions: usize,
    pub max_bytes: usize,
}
impl Default for CausalProjectionOptions {
    fn default() -> Self {
        Self {
            max_versions: 4096,
            max_bytes: 8_388_608,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CausalProjection {
    versions: Vec<CausalVersion>,
    selected: Vec<CausalVersion>,
}
fn invalid() -> LinkError {
    super::error("invalid causal projection")
}
impl CausalProjection {
    pub fn new(
        values: &[CausalVersion],
        policy: CausalConflictPolicy,
        options: CausalProjectionOptions,
    ) -> Result<Self, LinkError> {
        if !(1..=4096).contains(&options.max_versions)
            || !(1..=8_388_608).contains(&options.max_bytes)
            || values.len() > options.max_versions
        {
            return Err(invalid());
        }
        let mut groups: BTreeMap<u64, Vec<CausalVersion>> = BTreeMap::new();
        let mut dots: BTreeMap<CausalDot, String> = BTreeMap::new();
        let mut bytes = 0;
        for value in values {
            let wire = value.encode()?;
            bytes += wire.len();
            if bytes > options.max_bytes {
                return Err(invalid());
            }
            if dots
                .get(&value.dot)
                .is_some_and(|previous| previous != &wire)
            {
                return Err(invalid());
            }
            dots.insert(value.dot.clone(), wire);
            let group = groups.entry(value.address).or_default();
            if group.len() == 64 {
                return Err(invalid());
            }
            group.push(value.clone());
        }
        let mut versions = Vec::new();
        let mut selected = Vec::new();
        for group in groups.values() {
            let siblings = CausalVersion::merge(group, &[])?;
            if policy == CausalConflictPolicy::Reject && siblings.len() != 1 {
                return Err(super::error("conflict: concurrent causal siblings"));
            }
            selected.push(siblings.last().unwrap().clone());
            versions.extend(siblings);
        }
        Ok(Self { versions, selected })
    }
    pub fn versions(&self) -> Vec<CausalVersion> {
        self.versions.clone()
    }
    pub fn selected(&self) -> Vec<CausalVersion> {
        self.selected.clone()
    }
    pub fn rows(&self) -> Vec<Row<u64>> {
        self.selected
            .iter()
            .filter_map(|value| {
                value.payload.as_ref().map(|row| Row {
                    link: GenericLink::new(value.address, row.source, row.target),
                    name: row.name.clone(),
                })
            })
            .collect()
    }
}
