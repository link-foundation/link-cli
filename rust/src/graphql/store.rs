//! The rows of Deep over triplets.
//!
//! A row of `links` is a triplet with the external id `deep:<id>`, since Deep's
//! ids are not the addresses of the store. Deep's type `0` is a triplet of its
//! own, `DeepNoType`, because a triplet always has a type. A row of `strings`,
//! `numbers` or `objects` is a triplet `(row: DeepString owner 0)`, so deleting
//! the owner takes it along, and its text is the name `<row>:<text>`. Addresses
//! and values are also kept in memory, rebuilt from the links when opening.

use std::collections::{BTreeMap, HashMap, HashSet};

use anyhow::{anyhow, bail, Result};

use super::json::Json;
use super::plan::Position;
use crate::external_ids::LinksExternalIds;
use crate::link::Link;
use crate::named_type_links::NamedTypeLinks;
use crate::permissions::{PermissionPolicy, PermissionsDecorator};
use crate::triplets::{cascade_of, Triplet, TripletQuery, Triplets, TripletsOverDoublets};
use crate::undoable_links::UndoableLinksDecorator;

/// The tables of values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValueKind {
    String,
    Number,
    Object,
}

impl ValueKind {
    pub const ALL: [ValueKind; 3] = [ValueKind::String, ValueKind::Number, ValueKind::Object];

    pub fn table(self) -> &'static str {
        match self {
            ValueKind::String => "strings",
            ValueKind::Number => "numbers",
            ValueKind::Object => "objects",
        }
    }

    pub fn of_table(table: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.table() == table)
    }

    fn marker_name(self) -> &'static str {
        match self {
            ValueKind::String => "DeepString",
            ValueKind::Number => "DeepNumber",
            ValueKind::Object => "DeepObject",
        }
    }

    fn encode(self, value: &Json) -> String {
        match (self, value) {
            (ValueKind::String, Json::String(text)) => text.clone(),
            _ => value.write(),
        }
    }

    fn decode(self, text: &str) -> Json {
        match self {
            ValueKind::String => Json::str(text),
            _ => Json::parse(text).unwrap_or(Json::Null),
        }
    }
}

/// A row of `links`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkRow {
    pub id: i64,
    pub type_id: i64,
    pub from_id: i64,
    pub to_id: i64,
}

impl LinkRow {
    pub fn to_json(self) -> Json {
        Json::object(vec![
            ("id", Json::Int(self.id)),
            ("type_id", Json::Int(self.type_id)),
            ("from_id", Json::Int(self.from_id)),
            ("to_id", Json::Int(self.to_id)),
        ])
    }
}

#[derive(Debug, Clone)]
pub struct ValueRow {
    pub kind: ValueKind,
    pub owner: i64,
    pub value: Json,
}

pub struct DeepStore<'s> {
    triplets: TripletsOverDoublets<UndoableLinksDecorator<&'s mut (dyn NamedTypeLinks + 's)>>,
    policy: Option<PermissionPolicy>,
    created: HashMap<u32, HashSet<u32>>,
    ids: LinksExternalIds,
    to_triplet: HashMap<i64, u32>,
    to_deep: HashMap<u32, i64>,
    values: BTreeMap<u32, ValueRow>,
    by_owner: HashMap<(ValueKind, i64), u32>,
    markers: [u32; 3],
    no_type: u32,
    next_id: i64,
    /// `x-hasura-admin-secret`: the request is not judged by the policy.
    pub admin: bool,
    /// Bumped by every committed write, for what is derived from the rows.
    pub generation: u64,
}

fn named_point<T: Triplets + NamedTypeLinks>(triplets: &mut T, name: &str) -> Result<u32> {
    if let Some(found) = triplets.get_by_name(name)? {
        return Ok(found);
    }
    let point = triplets.create_triplet_point()?;
    triplets.set_name(point, name)?;
    Ok(point)
}

impl<'s> DeepStore<'s> {
    pub fn open(
        links: &'s mut (dyn NamedTypeLinks + 's),
        policy: Option<PermissionPolicy>,
    ) -> Result<Self> {
        let mut triplets =
            TripletsOverDoublets::with_named_markers(UndoableLinksDecorator::new(links))?;
        let no_type = named_point(&mut triplets, "DeepNoType")?;
        let mut markers = [0; 3];
        for (slot, kind) in markers.iter_mut().zip(ValueKind::ALL) {
            *slot = named_point(&mut triplets, kind.marker_name())?;
        }
        let mut store = Self {
            triplets,
            policy,
            created: HashMap::new(),
            ids: LinksExternalIds::new("deep")?,
            to_triplet: HashMap::new(),
            to_deep: HashMap::new(),
            values: BTreeMap::new(),
            by_owner: HashMap::new(),
            markers,
            no_type,
            next_id: 1,
            admin: false,
            generation: 0,
        };
        store.reload()?;
        Ok(store)
    }

    /// Rebuilds what is kept in memory from the links.
    fn reload(&mut self) -> Result<()> {
        self.to_triplet.clear();
        self.to_deep.clear();
        self.values.clear();
        self.by_owner.clear();
        self.next_id = 1;
        for triplet in self.triplets.each_triplet(TripletQuery::any()) {
            if let Some(id) = self.ids.id_of(&mut self.triplets, triplet.index)? {
                if let Ok(id) = id.parse::<i64>() {
                    self.to_triplet.insert(id, triplet.index);
                    self.to_deep.insert(triplet.index, id);
                    self.next_id = self.next_id.max(id + 1);
                }
            }
        }
        for (marker, kind) in self.markers.into_iter().zip(ValueKind::ALL) {
            for row in self.triplets.each_triplet(TripletQuery::by_type(marker)) {
                let Some(&owner) = self.to_deep.get(&row.from) else {
                    continue;
                };
                let name = self.triplets.get_name(row.index)?.unwrap_or_default();
                let text = name.split_once(':').map_or("", |(_, text)| text);
                self.by_owner.insert((kind, owner), row.index);
                self.values.insert(
                    row.index,
                    ValueRow {
                        kind,
                        owner,
                        value: kind.decode(text),
                    },
                );
            }
        }
        Ok(())
    }

    /// Runs `read` over the triplets as `subject` sees them.
    pub fn view<R>(&mut self, subject: u32, read: impl FnOnce(&mut dyn Triplets) -> R) -> R {
        match &mut self.policy {
            Some(_) if self.admin => read(&mut self.triplets),
            None => read(&mut self.triplets),
            Some(policy) => {
                let created = self.created.remove(&subject).unwrap_or_default();
                let mut view = PermissionsDecorator::new(&mut self.triplets, policy, subject)
                    .with_created(created);
                let result = read(&mut view);
                let created = view.created().clone();
                self.created.insert(subject, created);
                result
            }
        }
    }

    pub fn has_policy(&self) -> bool {
        self.policy.is_some()
    }

    pub fn triplet_of(&self, id: i64) -> Option<u32> {
        self.to_triplet.get(&id).copied()
    }

    /// The address a Deep id stands for in `position` of a query.
    pub fn address(&self, position: Position, id: i64) -> Option<u32> {
        if id == 0 && position == Position::Type {
            return Some(self.no_type);
        }
        self.triplet_of(id)
    }

    pub fn deep_of(&self, triplet: u32) -> Option<i64> {
        self.to_deep.get(&triplet).copied()
    }

    /// The row of a triplet, if it is a row of `links`.
    pub fn row_of(&self, triplet: &Triplet) -> Option<LinkRow> {
        let id = self.deep_of(triplet.index)?;
        let part = |address: u32| {
            if address == 0 {
                Some(0)
            } else {
                self.deep_of(address)
            }
        };
        let type_id = if triplet.type_id == self.no_type {
            0
        } else {
            self.deep_of(triplet.type_id)?
        };
        Some(LinkRow {
            id,
            type_id,
            from_id: part(triplet.from)?,
            to_id: part(triplet.to)?,
        })
    }

    pub fn values(&self) -> impl Iterator<Item = (u32, &ValueRow)> {
        self.values.iter().map(|(row, value)| (*row, value))
    }

    pub fn value_of_owner(&self, kind: ValueKind, owner: i64) -> Option<(u32, &ValueRow)> {
        let row = *self.by_owner.get(&(kind, owner))?;
        self.values.get(&row).map(|value| (row, value))
    }

    pub fn value(&self, row: u32) -> Option<&ValueRow> {
        self.values.get(&row)
    }

    pub fn reserve(&mut self, count: i64) -> Vec<i64> {
        let first = self.next_id;
        self.next_id += count.max(0);
        (first..self.next_id).collect()
    }

    pub fn begin(&mut self) {
        self.triplets.links_mut().begin();
    }

    pub fn commit(&mut self) -> Result<()> {
        self.triplets.links_mut().commit();
        self.generation += 1;
        self.triplets.save()
    }

    pub fn rollback(&mut self) -> Result<()> {
        self.triplets.links_mut().rollback()?;
        self.reload()
    }

    fn type_address(&self, type_id: i64) -> Result<u32> {
        self.address(Position::Type, type_id)
            .ok_or_else(|| anyhow!("Foreign key violation: no link {type_id} for type_id."))
    }

    fn part_address(&self, id: i64, column: &str) -> Result<u32> {
        if id == 0 {
            return Ok(0);
        }
        self.triplet_of(id)
            .ok_or_else(|| anyhow!("Foreign key violation: no link {id} for {column}."))
    }

    /// Inserts a row of `links`; a part may name the id being inserted.
    pub fn create_link(
        &mut self,
        subject: u32,
        id: Option<i64>,
        type_id: i64,
        from_id: i64,
        to_id: i64,
    ) -> Result<i64> {
        let id = id.unwrap_or(self.next_id);
        if self.to_triplet.contains_key(&id) {
            bail!("Uniqueness violation: duplicate key value violates unique constraint \"links_pkey\": {id}.");
        }
        let triplet = if type_id == id || from_id == id || to_id == id {
            let point = self.view(subject, |view| view.create_triplet_point())?;
            self.install(id, point)?;
            self.update_link(subject, id, type_id, from_id, to_id)?;
            point
        } else {
            let (type_address, from, to) = (
                self.type_address(type_id)?,
                self.part_address(from_id, "from_id")?,
                self.part_address(to_id, "to_id")?,
            );
            let triplet = self.view(subject, |view| view.create_triplet(type_address, from, to))?;
            self.install(id, triplet)?;
            triplet
        };
        let _ = triplet;
        Ok(id)
    }

    fn install(&mut self, id: i64, triplet: u32) -> Result<()> {
        self.ids
            .bind(&mut self.triplets, &id.to_string(), triplet)?;
        self.to_triplet.insert(id, triplet);
        self.to_deep.insert(triplet, id);
        self.next_id = self.next_id.max(id + 1);
        Ok(())
    }

    pub fn update_link(
        &mut self,
        subject: u32,
        id: i64,
        type_id: i64,
        from_id: i64,
        to_id: i64,
    ) -> Result<()> {
        let index = self.part_address(id, "id")?;
        let (type_address, from, to) = (
            self.type_address(type_id)?,
            self.part_address(from_id, "from_id")?,
            self.part_address(to_id, "to_id")?,
        );
        self.view(subject, |view| {
            view.update_triplet(index, type_address, from, to)
        })?;
        Ok(())
    }

    /// Deletes a row of `links` and all that uses it; returns the ids deleted.
    pub fn delete_link(&mut self, subject: u32, id: i64) -> Result<Vec<i64>> {
        let index = self.part_address(id, "id")?;
        let Some(first) = self.triplets.get_triplet(index) else {
            bail!("Link {id} is not a triplet.");
        };
        let cascade = cascade_of(&mut self.triplets, first);
        let mut deleted = Vec::new();
        // The external ids and value names point at the rows: they go first.
        for triplet in &cascade {
            if let Some(deep) = self.deep_of(triplet.index) {
                self.ids.unbind(&mut self.triplets, &deep.to_string())?;
                deleted.push(deep);
            }
            if self.values.contains_key(&triplet.index) {
                self.triplets.remove_name(triplet.index)?;
            }
        }
        self.view(subject, |view| view.delete_triplet(index))?;
        for triplet in &cascade {
            if let Some(deep) = self.to_deep.remove(&triplet.index) {
                self.to_triplet.remove(&deep);
            }
            if let Some(row) = self.values.remove(&triplet.index) {
                self.by_owner.remove(&(row.kind, row.owner));
            }
        }
        Ok(deleted)
    }

    /// Whether `subject` may change what hangs on the link `owner`.
    fn require_owner_writable(&mut self, subject: u32, owner: i64) -> Result<u32> {
        let address = self.part_address(owner, "link_id")?;
        if let (Some(policy), false) = (&mut self.policy, self.admin) {
            let outer = self.triplets.get_link(address);
            let mut view = PermissionsDecorator::new(&mut self.triplets, policy, subject);
            let allowed = outer.is_some_and(|outer: Link| view.can_write_triplet(&outer));
            if !allowed {
                bail!("Forbidden: {subject} may not write the value of {owner}.");
            }
        }
        Ok(address)
    }

    pub fn create_value(
        &mut self,
        subject: u32,
        kind: ValueKind,
        owner: i64,
        value: Json,
    ) -> Result<u32> {
        let address = self.require_owner_writable(subject, owner)?;
        if self.by_owner.contains_key(&(kind, owner)) {
            bail!(
                "Uniqueness violation: link {owner} already has a row in {}.",
                kind.table()
            );
        }
        let marker = self.markers[ValueKind::ALL.iter().position(|k| *k == kind).unwrap_or(0)];
        let row = self.triplets.create_triplet(marker, address, 0)?;
        self.triplets
            .set_name(row, &format!("{row}:{}", kind.encode(&value)))?;
        self.by_owner.insert((kind, owner), row);
        self.values.insert(row, ValueRow { kind, owner, value });
        Ok(row)
    }

    pub fn update_value(&mut self, subject: u32, row: u32, value: Json) -> Result<()> {
        let Some(found) = self.values.get(&row).cloned() else {
            bail!("No value row {row}.");
        };
        self.require_owner_writable(subject, found.owner)?;
        self.triplets
            .set_name(row, &format!("{row}:{}", found.kind.encode(&value)))?;
        if let Some(stored) = self.values.get_mut(&row) {
            stored.value = value;
        }
        Ok(())
    }

    pub fn delete_value(&mut self, subject: u32, row: u32) -> Result<()> {
        let Some(found) = self.values.get(&row).cloned() else {
            bail!("No value row {row}.");
        };
        self.require_owner_writable(subject, found.owner)?;
        self.triplets.remove_name(row)?;
        self.triplets.delete_triplet(row)?;
        self.values.remove(&row);
        self.by_owner.remove(&(found.kind, found.owner));
        Ok(())
    }

    /// Loads rows `{id, type_id, from_id, to_id, string?, number?, object?}`,
    /// such as a dump of `@deep-foundation/core`; rows already there are left
    /// alone, so loading twice changes nothing. Ids may refer forward, so all
    /// the points are made first.
    pub fn load_package(&mut self, rows: &Json) -> Result<usize> {
        let rows = rows.as_list();
        let int = |row: &Json, key: &str| row.get(key).and_then(Json::as_i64).unwrap_or(0);
        let fresh: Vec<&Json> = rows
            .iter()
            .copied()
            .filter(|row| self.triplet_of(int(row, "id")).is_none())
            .collect();
        for row in &fresh {
            let point = self.triplets.create_triplet_point()?;
            self.install(int(row, "id"), point)?;
        }
        for row in &fresh {
            let (index, type_address, from, to) = (
                self.part_address(int(row, "id"), "id")?,
                self.type_address(int(row, "type_id"))?,
                self.part_address(int(row, "from_id"), "from_id")?,
                self.part_address(int(row, "to_id"), "to_id")?,
            );
            self.triplets
                .update_triplet(index, type_address, from, to)?;
        }
        for row in &fresh {
            for (key, kind) in [
                ("string", ValueKind::String),
                ("number", ValueKind::Number),
                ("object", ValueKind::Object),
            ] {
                if let Some(value) = row.get(key) {
                    let address = self.part_address(int(row, "id"), "id")?;
                    let marker =
                        self.markers[ValueKind::ALL.iter().position(|k| *k == kind).unwrap_or(0)];
                    let value_row = self.triplets.create_triplet(marker, address, 0)?;
                    self.triplets
                        .set_name(value_row, &format!("{value_row}:{}", kind.encode(value)))?;
                    self.by_owner.insert((kind, int(row, "id")), value_row);
                    self.values.insert(
                        value_row,
                        ValueRow {
                            kind,
                            owner: int(row, "id"),
                            value: value.clone(),
                        },
                    );
                }
            }
        }
        self.triplets.save()?;
        self.generation += 1;
        Ok(fresh.len())
    }

    /// The grants of the policy as `(marker name, subject, object)`, in Deep ids
    /// (`None` where the rule says `Anything`).
    pub fn grants(&mut self) -> Vec<(bool, Option<i64>, Option<i64>)> {
        let Some(policy) = &mut self.policy else {
            return Vec::new();
        };
        use crate::permissions::AccessPolicy;
        let markers = policy.markers();
        let mut grants = Vec::new();
        for (write, marker) in [(false, markers.allow_read), (true, markers.allow_write)] {
            for (subject, object) in policy.granted(&mut self.triplets, marker) {
                let side = |side: u32| {
                    if side == markers.anything {
                        Some(None)
                    } else {
                        self.to_deep.get(&side).map(|id| Some(*id))
                    }
                };
                if let (Some(subject), Some(object)) = (side(subject), side(object)) {
                    grants.push((write, subject, object));
                }
            }
        }
        grants
    }
}
