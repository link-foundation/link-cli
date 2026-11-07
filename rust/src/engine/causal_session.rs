//! Ordinary owned links/names staging with explicit-token local publication.
use super::{
    causal::{CausalPayload, CausalVersion},
    causal_batch::{CausalBatch, CausalBatchEdit},
    causal_projection::{CausalConflictPolicy, CausalProjection, CausalProjectionOptions},
    cluster::AtomicJournalStore,
    Constraint, ConstraintValidation, Engine, EngineOptions, EngineTransaction, MemoryLog, Row,
    Rows, Snapshot, StateStructure,
};
use crate::{GenericLink, LinkError, LinksStorage};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
};

#[derive(Clone, Debug, Default)]
pub struct CausalWorkspaceOptions {
    pub state_structure: StateStructure,
    pub constraints: Vec<Constraint>,
    pub constraint_validation: ConstraintValidation,
    pub projection: CausalProjectionOptions,
    pub trace: bool,
}

/// A sealed standard-engine workspace with an independent reserved cursor.
/// No ordinary engine handle or unguarded allocating callback escapes.
pub struct CausalWorkspace {
    engine: Option<Engine<u64, MemoryLog>>,
    inner: Option<EngineTransaction<u64, MemoryLog>>,
    original: BTreeMap<u64, Row<u64>>,
    retained: BTreeMap<u64, Vec<CausalVersion>>,
    first: u64,
    last: u64,
    next: u128,
    reserved: VecDeque<u64>,
    final_rows: Vec<Row<u64>>,
}
fn invalid() -> LinkError {
    super::error("invalid or frozen causal workspace")
}
impl CausalWorkspace {
    /// Borrow the standard query/named-store interface with checked u64 writes
    /// and a u128 wildcard outside the complete stored address domain.
    pub fn named_links(&mut self) -> super::causal_named::CausalNamedLinks<'_> {
        super::causal_named::CausalNamedLinks { workspace: self }
    }

    pub(super) fn query_save(&mut self) -> Result<(), LinkError> {
        self.active().map(|_| ())
    }

    pub(super) fn reserve_query(
        &mut self,
        processor: &crate::GenericQueryProcessor<u128>,
        restriction: &crate::lino_link::LinoLink,
        substitution: &crate::lino_link::LinoLink,
    ) -> anyhow::Result<Option<std::collections::HashSet<u128>>> {
        self.active()?;
        let rows = self
            .rows()
            .into_iter()
            .map(|row| {
                let link = row.link;
                (
                    u128::from(link.index),
                    Row {
                        link: GenericLink::new(
                            link.index.into(),
                            link.source.into(),
                            link.target.into(),
                        ),
                        name: row.name,
                    },
                )
            })
            .collect();
        let mut scratch = super::query_scratch::QueryScratch {
            rows,
            next: self.next,
            last: self.last.into(),
            reserved: self.reserved.iter().copied().map(u128::from).collect(),
            allocated: Vec::new(),
            fresh_range: Some((
                self.first.into(),
                self.last.into(),
                self.retained.keys().copied().map(u128::from).collect(),
            )),
            limits: Some(crate::query_options::QueryLimits::causal()),
        };
        processor.preview(&mut scratch, restriction, substitution)?;
        self.next = self.next.max(scratch.next);
        for id in scratch.allocated {
            let id = u64::try_from(id).map_err(|_| invalid())?;
            if !self.reserved.contains(&id) {
                self.reserved.push_back(id);
            }
        }
        Ok(Some(scratch.rows.into_keys().collect()))
    }

    pub fn new(
        versions: &[CausalVersion],
        first: u64,
        last: u64,
        policy: CausalConflictPolicy,
        options: CausalWorkspaceOptions,
    ) -> Result<Self, LinkError> {
        if first == 0 || first > last {
            return Err(invalid());
        }
        let projection = CausalProjection::new(versions, policy, options.projection)?;
        let mut retained: BTreeMap<u64, Vec<CausalVersion>> = BTreeMap::new();
        for version in projection.versions() {
            if (first..=last).contains(&version.address) {
                return Err(invalid());
            }
            retained.entry(version.address).or_default().push(version);
        }
        let original: BTreeMap<_, _> = projection
            .rows()
            .into_iter()
            .map(|row| (row.link.index, row))
            .collect();
        let engine = Engine::open(
            MemoryLog::default(),
            EngineOptions {
                state_structure: options.state_structure,
                constraints: options.constraints,
                constraint_validation: options.constraint_validation,
                trace: options.trace,
                ..Default::default()
            },
        )?;
        {
            let mut state = engine.lock()?;
            let mut rows = Rows::new(options.state_structure);
            for (id, row) in &original {
                rows.insert(*id, row.clone());
            }
            state.validate(&rows)?;
            if options.constraint_validation == ConstraintValidation::Indexed {
                state.constraint_index = super::constraint_index::ConstraintIndex::build(
                    &state.options.constraints,
                    &rows,
                )?;
            }
            // Ordinary journal next must exceed every visible address, even
            // when the independent fresh reservation lies below a remote row.
            let next = original
                .last_key_value()
                .map_or(1, |(id, _)| u128::from(*id) + 1);
            state.next = next;
            state.committed_next = next;
            state.current = Snapshot {
                revision: 0,
                rows: Arc::new(rows),
            };
        }
        let inner = Some(engine.begin()?);
        Ok(Self {
            engine: Some(engine),
            inner,
            original,
            retained,
            first,
            last,
            next: u128::from(first),
            reserved: VecDeque::new(),
            final_rows: Vec::new(),
        })
    }
    fn active(&mut self) -> Result<&mut EngineTransaction<u64, MemoryLog>, LinkError> {
        self.inner.as_mut().ok_or_else(invalid)
    }
    pub fn rows(&self) -> Vec<Row<u64>> {
        self.inner.as_ref().map_or_else(
            || self.final_rows.clone(),
            |tx| {
                tx.all_links()
                    .into_iter()
                    .map(|link| Row {
                        name: tx.get_name(link.index),
                        link,
                    })
                    .collect()
            },
        )
    }
    pub fn get_name(&self, id: u64) -> Option<String> {
        self.inner.as_ref().map_or_else(
            || {
                self.final_rows
                    .iter()
                    .find(|row| row.link.index == id)
                    .and_then(|row| row.name.clone())
            },
            |tx| tx.get_name(id),
        )
    }
    pub fn get_by_name(&self, name: &str) -> Option<u64> {
        self.inner.as_ref().map_or_else(
            || {
                self.final_rows
                    .iter()
                    .find(|row| row.name.as_deref() == Some(name))
                    .map(|row| row.link.index)
            },
            |tx| tx.get_by_name(name),
        )
    }
    pub fn set_name(&mut self, id: u64, name: Option<String>) -> Result<(), LinkError> {
        self.active()?.set_name(id, name)
    }
    pub fn finish(&mut self) -> Result<Vec<CausalBatchEdit>, LinkError> {
        let mut tx = self.inner.take().ok_or_else(invalid)?;
        let engine = self.engine.take().ok_or_else(invalid)?;
        // All ordinary constraints and the selected immutable representation
        // are validated through the same commit path as a standard session.
        tx.commit()?;
        self.final_rows = engine
            .snapshot()?
            .iter()
            .map(|(_, row)| row.clone())
            .collect();
        let final_rows: BTreeMap<_, _> = self
            .final_rows
            .iter()
            .map(|row| (row.link.index, row))
            .collect();
        let keys: BTreeSet<_> = self
            .original
            .keys()
            .chain(final_rows.keys())
            .copied()
            .collect();
        let edits: Vec<_> = keys
            .into_iter()
            .filter_map(|id| {
                let before = self.original.get(&id);
                let after = final_rows.get(&id).copied();
                if before == after {
                    return None;
                }
                Some(CausalBatchEdit {
                    address: id,
                    observed: self.retained.get(&id).cloned().unwrap_or_default(),
                    payload: after.map(|row| CausalPayload {
                        source: row.link.source,
                        target: row.link.target,
                        name: row.name.clone(),
                    }),
                })
            })
            .collect();
        self.original.clear();
        self.retained.clear();
        self.reserved.clear();
        if edits.len() > 64 {
            return Err(invalid());
        }
        super::causal_batch::normalize(&edits)?;
        Ok(edits)
    }
    pub fn rollback(&mut self) {
        self.inner.take();
        self.engine.take();
        self.original.clear();
        self.retained.clear();
        self.reserved.clear();
        self.final_rows.clear();
    }
}
impl LinksStorage<u64> for CausalWorkspace {
    fn create_link(&mut self, source: u64, target: u64) -> Result<u64, LinkError> {
        self.active()?;
        let id = if let Some(id) = self.reserved.front().copied() {
            id
        } else {
            if self.next > u128::from(self.last) {
                return Err(invalid());
            }
            self.next as u64
        };
        let tx = self.active()?;
        tx.ensure_link_created(id)?;
        tx.update_link(id, source, target)?;
        if self.reserved.front() == Some(&id) {
            self.reserved.pop_front();
        } else {
            self.next += 1;
        }
        Ok(id)
    }
    fn ensure_link_created(&mut self, id: u64) -> Result<u64, LinkError> {
        if id == 0 || (!(self.first..=self.last).contains(&id) && !self.retained.contains_key(&id))
        {
            return Err(invalid());
        }
        self.active()?.ensure_link_created(id)?;
        if (self.first..=self.last).contains(&id) {
            self.next = self.next.max(u128::from(id) + 1);
        }
        self.reserved.retain(|pending| *pending != id);
        Ok(id)
    }
    fn get_link(&self, id: u64) -> Option<GenericLink<u64>> {
        self.inner.as_ref().map_or_else(
            || {
                self.final_rows
                    .iter()
                    .find(|row| row.link.index == id)
                    .map(|row| row.link)
            },
            |tx| tx.get_link(id),
        )
    }
    fn update_link(
        &mut self,
        id: u64,
        source: u64,
        target: u64,
    ) -> Result<GenericLink<u64>, LinkError> {
        self.active()?.update_link(id, source, target)
    }
    fn delete_link(&mut self, id: u64) -> Result<GenericLink<u64>, LinkError> {
        self.active()?.delete_link(id)
    }
    fn all_links(&self) -> Vec<GenericLink<u64>> {
        self.inner.as_ref().map_or_else(
            || self.final_rows.iter().map(|row| row.link).collect(),
            LinksStorage::all_links,
        )
    }
    fn flush(&mut self) -> Result<(), LinkError> {
        Err(super::error(
            "causal publication requires an explicit retry token",
        ))
    }
}

#[derive(Clone, Debug)]
pub struct CausalPreparedBatch {
    pub token: String,
    pub generation: u64,
    pub edits: Vec<CausalBatchEdit>,
}
pub struct CausalSession<S: AtomicJournalStore> {
    batch: CausalBatch<S>,
    workspace: CausalWorkspace,
    generation: u64,
    prepared: Option<CausalPreparedBatch>,
    result: Option<Vec<CausalVersion>>,
    disposed: bool,
}
impl<S: AtomicJournalStore> CausalSession<S> {
    /// Standard named/query staging; preparation freezes the borrowed workspace.
    pub fn named_links(&mut self) -> super::causal_named::CausalNamedLinks<'_> {
        self.workspace.named_links()
    }

    pub fn new(
        mut batch: CausalBatch<S>,
        first: u64,
        last: u64,
        policy: CausalConflictPolicy,
        options: CausalWorkspaceOptions,
    ) -> Result<Self, LinkError> {
        let snapshot = batch.snapshot()?;
        let workspace = CausalWorkspace::new(&snapshot.versions, first, last, policy, options)?;
        Ok(Self {
            batch,
            workspace,
            generation: snapshot.generation,
            prepared: None,
            result: None,
            disposed: false,
        })
    }
    pub fn prepare(&mut self, token: &str) -> Result<CausalPreparedBatch, LinkError> {
        if self.disposed {
            return Err(invalid());
        }
        if let Some(request) = &self.prepared {
            if request.token != token {
                return Err(invalid());
            }
            return Ok(request.clone());
        }
        super::causal_batch::validate_token(token, 0)?;
        let edits = self.workspace.finish()?;
        super::causal_batch::validate_token(token, edits.len())?;
        let request = CausalPreparedBatch {
            token: token.into(),
            generation: self.generation,
            edits,
        };
        self.prepared = Some(request.clone());
        Ok(request)
    }
    pub fn prepared(&self) -> Option<CausalPreparedBatch> {
        self.prepared.clone()
    }
    pub fn commit(&mut self, token: &str) -> Result<Vec<CausalVersion>, LinkError> {
        let request = self.prepare(token)?;
        if let Some(result) = &self.result {
            return Ok(result.clone());
        }
        let result = self
            .batch
            .write(&request.token, request.generation, &request.edits)?;
        self.result = Some(result.clone());
        Ok(result)
    }
    pub fn rollback(&mut self) {
        self.workspace.rollback();
        self.disposed = true;
        self.result = None;
    }
    pub fn rows(&self) -> Vec<Row<u64>> {
        self.workspace.rows()
    }
    pub fn get_name(&self, id: u64) -> Option<String> {
        self.workspace.get_name(id)
    }
    pub fn get_by_name(&self, name: &str) -> Option<u64> {
        self.workspace.get_by_name(name)
    }
    pub fn set_name(&mut self, id: u64, name: Option<String>) -> Result<(), LinkError> {
        self.workspace.set_name(id, name)
    }
}
impl<S: AtomicJournalStore> LinksStorage<u64> for CausalSession<S> {
    fn create_link(&mut self, source: u64, target: u64) -> Result<u64, LinkError> {
        self.workspace.create_link(source, target)
    }
    fn ensure_link_created(&mut self, id: u64) -> Result<u64, LinkError> {
        self.workspace.ensure_link_created(id)
    }
    fn get_link(&self, id: u64) -> Option<GenericLink<u64>> {
        self.workspace.get_link(id)
    }
    fn update_link(
        &mut self,
        id: u64,
        source: u64,
        target: u64,
    ) -> Result<GenericLink<u64>, LinkError> {
        self.workspace.update_link(id, source, target)
    }
    fn delete_link(&mut self, id: u64) -> Result<GenericLink<u64>, LinkError> {
        self.workspace.delete_link(id)
    }
    fn all_links(&self) -> Vec<GenericLink<u64>> {
        self.workspace.all_links()
    }
    fn flush(&mut self) -> Result<(), LinkError> {
        self.workspace.flush()
    }
}
