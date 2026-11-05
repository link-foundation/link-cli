//! `insert_T`, `update_T` and `delete_T` on `links` and the tables of values.

use anyhow::anyhow;

use super::document::Field;
use super::execute::{Exec, GqlError, GqlResult};
use super::json::Json;
use super::select::primary_key;
use super::store::{LinkRow, ValueKind};

fn writable(table: &str) -> bool {
    table == "links" || ValueKind::of_table(table).is_some()
}

impl<'a> Exec<'a, '_> {
    /// The mutation `field` names, `None` when it names none.
    pub fn mutation_field(&mut self, field: &'a Field) -> GqlResult<Option<Json>> {
        let name = field.name.as_str();
        let vars = self.variables;
        if let Some(table) = name
            .strip_prefix("insert_")
            .filter(|t| writable(t.trim_end_matches("_one")))
        {
            let single = table.ends_with("_one");
            let table = table.trim_end_matches("_one");
            let objects = if single {
                vec![field.argument("object", vars).unwrap_or(Json::Null)]
            } else {
                match field.argument("objects", vars) {
                    Some(Json::Array(items)) => items,
                    Some(one) => vec![one],
                    None => Vec::new(),
                }
            };
            let mut rows = Vec::new();
            for object in &objects {
                rows.push(self.insert(table, object, &[])?);
            }
            self.memo.clear();
            return Ok(Some(if single {
                match rows.first() {
                    Some(row) => self.project(table, row, &field.selections)?,
                    None => Json::Null,
                }
            } else {
                self.response(table, &rows, field)?
            }));
        }
        if let Some(rest) = name.strip_prefix("update_") {
            let (table, by_pk) = match rest.strip_suffix("_by_pk") {
                Some(table) => (table, true),
                None => (rest, false),
            };
            if writable(table) {
                let rows = self.target_rows(table, field, by_pk)?;
                let mut updated = Vec::new();
                for row in &rows {
                    updated.push(self.update(table, row, field)?);
                }
                self.memo.clear();
                return Ok(Some(self.finish(table, &updated, field, by_pk)?));
            }
        }
        if let Some(rest) = name.strip_prefix("delete_") {
            let (table, by_pk) = match rest.strip_suffix("_by_pk") {
                Some(table) => (table, true),
                None => (rest, false),
            };
            if writable(table) {
                let rows = self.target_rows(table, field, by_pk)?;
                // What is returned is read before it is gone.
                let response = self.finish(table, &rows, field, by_pk)?;
                for row in &rows {
                    let id = row.get("id").and_then(Json::as_i64).unwrap_or(0);
                    let subject = self.subject;
                    if table == "links" {
                        // Taken along by an earlier row of this delete, it is done.
                        if self.store.triplet_of(id).is_some() {
                            self.store
                                .delete_link(subject, id)
                                .map_err(GqlError::write)?;
                        }
                    } else if self.store.value(id as u32).is_some() {
                        self.store
                            .delete_value(subject, id as u32)
                            .map_err(GqlError::write)?;
                    }
                }
                self.memo.clear();
                return Ok(Some(response));
            }
        }
        if name == "reserve" {
            let count = field
                .argument("count", vars)
                .and_then(|c| c.as_i64())
                .unwrap_or(0);
            let ids = self.store.reserve(count);
            let mut out = Vec::new();
            for item in self.flatten(&field.selections, "ReserveResponse")? {
                let value = match item.name.as_str() {
                    "__typename" => Json::str("ReserveResponse"),
                    _ => Json::Array(ids.iter().map(|id| Json::Int(*id)).collect()),
                };
                out.push((item.response_key().to_string(), value));
            }
            return Ok(Some(Json::Object(out)));
        }
        Ok(None)
    }

    fn target_rows(&mut self, table: &str, field: &'a Field, by_pk: bool) -> GqlResult<Vec<Json>> {
        let vars = self.variables;
        let condition = if by_pk {
            let key = field
                .argument("pk_columns", vars)
                .and_then(|columns| columns.get(primary_key(table)).cloned())
                .or_else(|| field.argument(primary_key(table), vars))
                .unwrap_or(Json::Null);
            Json::Object(vec![(
                primary_key(table).to_string(),
                Json::object(vec![("_eq", key)]),
            )])
        } else {
            field.argument("where", vars).unwrap_or(Json::Null)
        };
        self.rows(table, &condition)
    }

    fn finish(
        &mut self,
        table: &str,
        rows: &[Json],
        field: &'a Field,
        by_pk: bool,
    ) -> GqlResult<Json> {
        if by_pk {
            return match rows.first() {
                Some(row) => self.project(table, row, &field.selections),
                None => Ok(Json::Null),
            };
        }
        self.response(table, rows, field)
    }

    /// `{affected_rows returning {..}}`.
    fn response(&mut self, table: &str, rows: &[Json], field: &'a Field) -> GqlResult<Json> {
        let typename = format!("{table}_mutation_response");
        let mut out = Vec::new();
        for item in self.flatten(&field.selections, &typename)? {
            let value = match item.name.as_str() {
                "__typename" => Json::String(typename.clone()),
                "affected_rows" => Json::Int(rows.len() as i64),
                "returning" => self.project_all(table, rows, &item.selections)?,
                other => {
                    return Err(GqlError::validation(
                        format!("field '{other}' not found in type: '{typename}'"),
                        format!("$.selectionSet.{}.{other}", field.name),
                    ))
                }
            };
            out.push((item.response_key().to_string(), value));
        }
        Ok(Json::Object(out))
    }

    /// The id a column or its nested `{data: ..}` object gives.
    fn column_id(
        &mut self,
        object: &Json,
        column: &str,
        relation: &str,
        given: &[(&str, i64)],
    ) -> GqlResult<i64> {
        if let Some((_, id)) = given.iter().find(|(name, _)| *name == column) {
            return Ok(*id);
        }
        if let Some(nested) = object.get(relation).and_then(|nested| nested.get("data")) {
            let row = self.insert("links", nested, &[])?;
            return Ok(row.get("id").and_then(Json::as_i64).unwrap_or(0));
        }
        Ok(object.get(column).and_then(Json::as_i64).unwrap_or(0))
    }

    /// Inserts one object and its nested objects; `given` are the columns the
    /// parent of a nested array fixes.
    fn insert(&mut self, table: &str, object: &Json, given: &[(&str, i64)]) -> GqlResult<Json> {
        if let Some(kind) = ValueKind::of_table(table) {
            let owner = match given.iter().find(|(name, _)| *name == "link_id") {
                Some((_, owner)) => *owner,
                None => object.get("link_id").and_then(Json::as_i64).unwrap_or(0),
            };
            let value = object.get("value").cloned().unwrap_or(Json::Null);
            let subject = self.subject;
            let row = self
                .store
                .create_value(subject, kind, owner, value)
                .map_err(GqlError::write)?;
            let found = self
                .store
                .value(row)
                .ok_or_else(|| GqlError::write("A value row is gone."))?;
            return Ok(Self::value_json(row, found));
        }
        let type_id = self.column_id(object, "type_id", "type", given)?;
        let from_id = self.column_id(object, "from_id", "from", given)?;
        let to_id = self.column_id(object, "to_id", "to", given)?;
        let explicit = object.get("id").and_then(Json::as_i64);
        let subject = self.subject;
        let id = self
            .store
            .create_link(subject, explicit, type_id, from_id, to_id)
            .map_err(GqlError::write)?;
        for (key, kind) in [
            ("string", ValueKind::String),
            ("number", ValueKind::Number),
            ("object", ValueKind::Object),
        ] {
            if let Some(nested) = object.get(key).and_then(|nested| nested.get("data")) {
                self.insert(kind.table(), nested, &[("link_id", id)])?;
            }
        }
        for (key, column) in [("in", "to_id"), ("out", "from_id"), ("typed", "type_id")] {
            if let Some(nested) = object.get(key).and_then(|nested| nested.get("data")) {
                let items = match nested {
                    Json::Array(items) => items.iter().collect(),
                    one => vec![one],
                };
                for item in items {
                    self.insert("links", item, &[(column, id)])?;
                }
            }
        }
        Ok(LinkRow {
            id,
            type_id,
            from_id,
            to_id,
        }
        .to_json())
    }

    fn update(&mut self, table: &str, row: &Json, field: &'a Field) -> GqlResult<Json> {
        let vars = self.variables;
        let set = field.argument("_set", vars).unwrap_or(Json::Null);
        let increase = field.argument("_inc", vars).unwrap_or(Json::Null);
        let subject = self.subject;
        let id = row.get("id").and_then(Json::as_i64).unwrap_or(0);
        if table == "links" {
            let column = |name: &str| -> i64 {
                let base = set
                    .get(name)
                    .and_then(Json::as_i64)
                    .or_else(|| row.get(name).and_then(Json::as_i64))
                    .unwrap_or(0);
                base + increase.get(name).and_then(Json::as_i64).unwrap_or(0)
            };
            let next = LinkRow {
                id,
                type_id: column("type_id"),
                from_id: column("from_id"),
                to_id: column("to_id"),
            };
            self.store
                .update_link(subject, id, next.type_id, next.from_id, next.to_id)
                .map_err(GqlError::write)?;
            return Ok(next.to_json());
        }
        let current = row.get("value").cloned().unwrap_or(Json::Null);
        let value = match (set.get("value"), increase.get("value")) {
            (Some(value), _) => value.clone(),
            (None, Some(step)) => {
                Json::Float(current.as_f64().unwrap_or(0.0) + step.as_f64().unwrap_or(0.0))
            }
            _ => current,
        };
        self.store
            .update_value(subject, id as u32, value)
            .map_err(|error| GqlError::write(anyhow!(error)))?;
        let found = self
            .store
            .value(id as u32)
            .ok_or_else(|| GqlError::write("A value row is gone."))?;
        Ok(Self::value_json(id as u32, found))
    }
}
