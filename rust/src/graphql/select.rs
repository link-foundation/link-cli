//! Reading tables: `where` over plans and checks, ordering, relations,
//! aggregates and the projection of a selection.

use std::collections::{BTreeSet, HashSet};
use std::rc::Rc;

use super::document::{Field, Selection};
use super::execute::{Exec, GqlError, GqlResult};
use super::json::Json;
use super::plan::{compile, Plan};
use super::store::{LinkRow, ValueKind};
use super::tables::{columns, compare, passes, relation, Relation, TABLES};
use crate::triplets::{match_triplets_query, Triplet, TripletQuery};

impl<'a> Exec<'a, '_> {
    /// The root fields `T`, `T_by_pk` and `T_aggregate`.
    pub fn select_field(&mut self, field: &'a Field) -> GqlResult<Option<Json>> {
        let name = field.name.as_str();
        if let Some(table) = TABLES.iter().find(|table| **table == name) {
            let rows = self.select(table, field)?;
            return Ok(Some(self.project_all(table, &rows, &field.selections)?));
        }
        if let Some(table) = name
            .strip_suffix("_by_pk")
            .filter(|table| TABLES.contains(table))
        {
            let key = primary_key(table);
            let id = field.argument(key, self.variables).unwrap_or(Json::Null);
            let condition = Json::Object(vec![(key.to_string(), Json::object(vec![("_eq", id)]))]);
            let rows = self.rows(table, &condition)?;
            return Ok(Some(match rows.first() {
                Some(row) => self.project(table, row, &field.selections)?,
                None => Json::Null,
            }));
        }
        if let Some(table) = name
            .strip_suffix("_aggregate")
            .filter(|table| TABLES.contains(table))
        {
            let rows = self.select(table, field)?;
            return Ok(Some(self.aggregate(table, rows, field)?));
        }
        Ok(None)
    }

    /// The rows the arguments of `field` select.
    fn select(&mut self, table: &str, field: &'a Field) -> GqlResult<Vec<Json>> {
        let condition = field
            .argument("where", self.variables)
            .unwrap_or(Json::Null);
        let rows = self.rows(table, &condition)?;
        self.shape(table, rows, field)
    }

    /// Applies `order_by`, `distinct_on`, `offset` and `limit`.
    fn shape(
        &mut self,
        table: &str,
        mut rows: Vec<Json>,
        field: &'a Field,
    ) -> GqlResult<Vec<Json>> {
        let distinct = field
            .argument("distinct_on", self.variables)
            .unwrap_or(Json::Null);
        let distinct: Vec<String> = match &distinct {
            Json::String(column) => vec![column.clone()],
            other => other
                .as_list()
                .iter()
                .filter_map(|c| c.as_str().map(str::to_string))
                .collect(),
        };
        let order = field
            .argument("order_by", self.variables)
            .unwrap_or(Json::Null);
        let mut specs = Vec::new();
        for item in order_items(&order) {
            flatten_order(&item, &mut Vec::new(), &mut specs);
        }
        if !specs.is_empty() {
            let mut keyed = Vec::with_capacity(rows.len());
            for row in rows {
                let mut key = Vec::with_capacity(specs.len());
                for (path, _) in &specs {
                    key.push(self.sort_value(table, &row, path)?);
                }
                keyed.push((key, row));
            }
            keyed.sort_by(|(left, _), (right, _)| {
                for ((left, right), (_, direction)) in left.iter().zip(right).zip(&specs) {
                    let found = order_values(left, right, direction);
                    if found != std::cmp::Ordering::Equal {
                        return found;
                    }
                }
                std::cmp::Ordering::Equal
            });
            rows = keyed.into_iter().map(|(_, row)| row).collect();
        }
        if !distinct.is_empty() {
            let mut seen = HashSet::new();
            rows.retain(|row| {
                let key: Vec<String> = distinct
                    .iter()
                    .map(|column| row.get(column).map_or_else(String::new, Json::write))
                    .collect();
                seen.insert(key)
            });
        }
        let offset = field
            .argument("offset", self.variables)
            .and_then(|o| o.as_i64())
            .unwrap_or(0)
            .max(0) as usize;
        let rows = rows.into_iter().skip(offset);
        Ok(
            match field
                .argument("limit", self.variables)
                .and_then(|l| l.as_i64())
            {
                Some(limit) => rows.take(limit.max(0) as usize).collect(),
                None => rows.collect(),
            },
        )
    }

    fn sort_value(&mut self, table: &str, row: &Json, path: &[String]) -> GqlResult<Json> {
        let Some((first, rest)) = path.split_first() else {
            return Ok(Json::Null);
        };
        if let Some(found) = relation(table, first) {
            let local = row.get(found.local).cloned().unwrap_or(Json::Null);
            let related = self.related(found, &local)?;
            return match related.first() {
                Some(next) => self.sort_value(found.target, next, rest),
                None => Ok(Json::Null),
            };
        }
        self.column_value(table, row, first)
    }

    /// The rows of `table` passing `condition`.
    pub fn rows(&mut self, table: &str, condition: &Json) -> GqlResult<Vec<Json>> {
        if table != "links" {
            let mut rows = self.table_rows(table)?;
            let mut kept = Vec::new();
            for row in rows.drain(..) {
                if self.matches(table, &row, condition)? {
                    kept.push(row);
                }
            }
            return Ok(kept);
        }
        let store = &*self.store;
        let compiled = compile(condition, &|position, id| store.address(position, id));
        let mut found = self.link_rows_of(&compiled.plan)?;
        found.sort_by_key(|row| row.id);
        let mut rows = Vec::with_capacity(found.len());
        for row in found {
            let row = row.to_json();
            if compiled.exact || self.matches("links", &row, condition)? {
                rows.push(row);
            }
        }
        Ok(rows)
    }

    fn link_rows_of(&mut self, plan: &Plan) -> GqlResult<Vec<LinkRow>> {
        let subject = self.subject;
        let addresses = self.addresses(plan)?;
        let triplets: Vec<Triplet> = self.store.view(subject, |view| {
            addresses
                .iter()
                .filter_map(|address| view.get_triplet(*address))
                .collect()
        });
        Ok(triplets
            .iter()
            .filter_map(|triplet| self.store.row_of(triplet))
            .collect())
    }

    /// The addresses of the rows a plan stands for: this is where a `where`
    /// becomes substitution queries.
    fn addresses(&mut self, plan: &Plan) -> GqlResult<BTreeSet<u32>> {
        let subject = self.subject;
        Ok(match plan {
            Plan::Empty => BTreeSet::new(),
            Plan::All => {
                let all = self
                    .store
                    .view(subject, |view| view.each_triplet(TripletQuery::any()));
                all.iter()
                    .map(|t| t.index)
                    .filter(|t| self.store.deep_of(*t).is_some())
                    .collect()
            }
            Plan::Ids(ids) => ids
                .iter()
                .filter_map(|id| self.store.triplet_of(*id))
                .collect(),
            Plan::Match(text) => {
                let found = self
                    .store
                    .view(subject, |view| match_triplets_query(view, text, "$i"))
                    .map_err(GqlError::write)?;
                found
                    .into_iter()
                    .filter(|t| self.store.deep_of(*t).is_some())
                    .collect()
            }
            Plan::Union(items) => {
                let mut all = BTreeSet::new();
                for item in items {
                    all.extend(self.addresses(item)?);
                }
                all
            }
            Plan::Intersect(items) => {
                let mut all: Option<BTreeSet<u32>> = None;
                for item in items {
                    let next = self.addresses(item)?;
                    all = Some(match all {
                        Some(all) => all.intersection(&next).copied().collect(),
                        None => next,
                    });
                }
                all.unwrap_or_default()
            }
            Plan::Minus(item) => {
                let without = self.addresses(item)?;
                let all = self.addresses(&Plan::All)?;
                all.difference(&without).copied().collect()
            }
        })
    }

    /// A column, or what a computed field `value` of `links` holds.
    fn column_value(&mut self, table: &str, row: &Json, column: &str) -> GqlResult<Json> {
        if table == "links" && column == "value" {
            let id = row.get("id").and_then(Json::as_i64).unwrap_or(0);
            for kind in ValueKind::ALL {
                if let Some((value_row, found)) = self.store.value_of_owner(kind, id) {
                    return Ok(Json::object(vec![
                        ("id", Json::Int(i64::from(value_row))),
                        ("value", found.value.clone()),
                    ]));
                }
            }
            return Ok(Json::Null);
        }
        Ok(row.get(column).cloned().unwrap_or(Json::Null))
    }

    /// Whether a row passes a `bool_exp`.
    pub fn matches(&mut self, table: &str, row: &Json, condition: &Json) -> GqlResult<bool> {
        let Json::Object(pairs) = condition else {
            return Ok(true);
        };
        for (key, value) in pairs {
            let passed = match key.as_str() {
                "_and" => {
                    let mut all = true;
                    for item in value.as_list() {
                        all = all && self.matches(table, row, item)?;
                    }
                    all
                }
                "_or" => {
                    let mut any = false;
                    for item in value.as_list() {
                        any = any || self.matches(table, row, item)?;
                    }
                    any
                }
                "_not" => !self.matches(table, row, value)?,
                _ => {
                    if let Some(found) = relation(table, key) {
                        let set = self.remote_set(found, value)?;
                        row.get(found.local)
                            .is_some_and(|local| set.contains(&local.write()))
                    } else if table == "links" && key == "value"
                        || columns(table).contains(&key.as_str())
                    {
                        let column = self.column_value(table, row, key)?;
                        let Json::Object(operators) = value else {
                            return Err(GqlError::validation(
                                "a comparison is an object",
                                format!("$.where.{key}"),
                            ));
                        };
                        let mut all = true;
                        for (operator, argument) in operators {
                            all = all
                                && passes(&column, operator, argument).map_err(|message| {
                                    GqlError::validation(message, format!("$.where.{key}"))
                                })?;
                        }
                        all
                    } else {
                        return Err(GqlError::validation(
                            format!("field '{key}' not found in type: '{table}_bool_exp'"),
                            format!("$.where.{key}"),
                        ));
                    }
                }
            };
            if !passed {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn remote_set(&mut self, found: Relation, condition: &Json) -> GqlResult<Rc<HashSet<String>>> {
        let key = format!("{}|{}|{}", found.target, found.remote, condition.write());
        if let Some(set) = self.memo.get(&key) {
            return Ok(Rc::clone(set));
        }
        let rows = self.rows(found.target, condition)?;
        let set: HashSet<String> = rows
            .iter()
            .filter_map(|row| row.get(found.remote))
            .filter(|value| !value.is_null())
            .map(Json::write)
            .collect();
        let set = Rc::new(set);
        self.memo.insert(key, Rc::clone(&set));
        Ok(set)
    }

    /// The rows at the far end of a relation, from the value of its local column.
    fn related(&mut self, found: Relation, local: &Json) -> GqlResult<Vec<Json>> {
        if local.is_null() {
            return Ok(Vec::new());
        }
        let id = local.as_i64().unwrap_or(0);
        let subject = self.subject;
        if found.target == "links" {
            let Some(address) = self.store.triplet_of(id) else {
                return Ok(Vec::new());
            };
            let query = match found.remote {
                "id" => TripletQuery::by_index(address),
                "type_id" => TripletQuery::by_type(address),
                "from_id" => TripletQuery::by_from(address),
                _ => TripletQuery::by_to(address),
            };
            let triplets = self.store.view(subject, |view| view.each_triplet(query));
            let mut rows: Vec<LinkRow> = triplets
                .iter()
                .filter_map(|t| self.store.row_of(t))
                .collect();
            rows.sort_by_key(|row| row.id);
            return Ok(rows.into_iter().map(LinkRow::to_json).collect());
        }
        if let (Some(kind), "link_id") = (ValueKind::of_table(found.target), found.remote) {
            let Some((row, value)) = self.store.value_of_owner(kind, id) else {
                return Ok(Vec::new());
            };
            return Ok(vec![Self::value_json(row, value)]);
        }
        let wanted = local.write();
        let rows = self.table_rows(found.target)?;
        Ok(rows
            .into_iter()
            .filter(|row| {
                row.get(found.remote)
                    .is_some_and(|value| value.write() == wanted)
            })
            .collect())
    }

    pub fn project_all(
        &mut self,
        table: &str,
        rows: &[Json],
        selections: &'a [Selection],
    ) -> GqlResult<Json> {
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            out.push(self.project(table, row, selections)?);
        }
        Ok(Json::Array(out))
    }

    /// A row under a selection.
    pub fn project(
        &mut self,
        table: &str,
        row: &Json,
        selections: &'a [Selection],
    ) -> GqlResult<Json> {
        let mut out = Vec::new();
        for field in self.flatten(selections, table)? {
            let name = field.name.as_str();
            let value = if name == "__typename" {
                Json::str(table)
            } else if let Some(found) = relation(table, name) {
                let local = row.get(found.local).cloned().unwrap_or(Json::Null);
                let rows = self.related(found, &local)?;
                let condition = field.argument("where", self.variables);
                let mut kept = Vec::with_capacity(rows.len());
                for candidate in rows {
                    let passes = match &condition {
                        Some(condition) => self.matches(found.target, &candidate, condition)?,
                        None => true,
                    };
                    if passes {
                        kept.push(candidate);
                    }
                }
                if found.many {
                    let shaped = self.shape(found.target, kept, field)?;
                    self.project_all(found.target, &shaped, &field.selections)?
                } else {
                    match kept.first() {
                        Some(first) => self.project(found.target, first, &field.selections)?,
                        None => Json::Null,
                    }
                }
            } else if let Some(found) = name
                .strip_suffix("_aggregate")
                .and_then(|base| relation(table, base))
            {
                let local = row.get(found.local).cloned().unwrap_or(Json::Null);
                let rows = self.related(found, &local)?;
                let condition = field.argument("where", self.variables);
                let mut kept = Vec::new();
                for candidate in rows {
                    if condition
                        .as_ref()
                        .map_or(Ok(true), |c| self.matches(found.target, &candidate, c))?
                    {
                        kept.push(candidate);
                    }
                }
                let shaped = self.shape(found.target, kept, field)?;
                self.aggregate(found.target, shaped, field)?
            } else if columns(table).contains(&name)
                || (table == "links" && name == "value")
                || row.get(name).is_some()
            {
                self.column_value(table, row, name)?
            } else {
                return Err(GqlError::validation(
                    format!("field '{name}' not found in type: '{table}'"),
                    format!("$.selectionSet.{table}.selectionSet.{name}"),
                ));
            };
            out.push((field.response_key().to_string(), value));
        }
        Ok(Json::Object(out))
    }

    /// `{aggregate {count max {..} ..} nodes {..}}`.
    fn aggregate(&mut self, table: &str, rows: Vec<Json>, field: &'a Field) -> GqlResult<Json> {
        let mut out = Vec::new();
        let typename = format!("{table}_aggregate");
        for item in self.flatten(&field.selections, &typename)? {
            let value = match item.name.as_str() {
                "__typename" => Json::String(typename.clone()),
                "nodes" => self.project_all(table, &rows, &item.selections)?,
                "aggregate" => self.aggregate_fields(table, &rows, item)?,
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

    fn aggregate_fields(
        &mut self,
        table: &str,
        rows: &[Json],
        field: &'a Field,
    ) -> GqlResult<Json> {
        let mut out = Vec::new();
        let typename = format!("{table}_aggregate_fields");
        for item in self.flatten(&field.selections, &typename)? {
            let name = item.name.as_str();
            let value = match name {
                "__typename" => Json::String(typename.clone()),
                "count" => {
                    let selected = item
                        .argument("columns", self.variables)
                        .unwrap_or(Json::Null);
                    let chosen: Vec<&Json> = match &selected {
                        Json::Null => Vec::new(),
                        Json::Array(items) => items.iter().collect(),
                        one => vec![one],
                    };
                    if chosen.is_empty() {
                        Json::Int(rows.len() as i64)
                    } else {
                        let key = |row: &Json| -> Option<String> {
                            let parts: Vec<String> = chosen
                                .iter()
                                .filter_map(|column| column.as_str())
                                .filter_map(|column| {
                                    row.get(column).filter(|v| !v.is_null()).map(Json::write)
                                })
                                .collect();
                            (parts.len() == chosen.len()).then(|| parts.join("|"))
                        };
                        let present = rows.iter().filter_map(key);
                        if matches!(
                            item.argument("distinct", self.variables),
                            Some(Json::Bool(true))
                        ) {
                            Json::Int(present.collect::<HashSet<_>>().len() as i64)
                        } else {
                            Json::Int(present.count() as i64)
                        }
                    }
                }
                "max" | "min" | "sum" | "avg" => {
                    let inner = format!("{table}_{name}_fields");
                    let mut fields = Vec::new();
                    for column in self.flatten(&item.selections, &inner)? {
                        let value = if column.name == "__typename" {
                            Json::String(inner.clone())
                        } else {
                            summarise(name, rows, &column.name)
                        };
                        fields.push((column.response_key().to_string(), value));
                    }
                    Json::Object(fields)
                }
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
}

pub fn primary_key(table: &str) -> &'static str {
    match table {
        "selectors" => "item_id",
        "handlers" => "handler_id",
        "can" => "rule_id",
        _ => "id",
    }
}

fn summarise(operation: &str, rows: &[Json], column: &str) -> Json {
    let values: Vec<&Json> = rows
        .iter()
        .filter_map(|row| row.get(column))
        .filter(|v| !v.is_null())
        .collect();
    if values.is_empty() {
        return Json::Null;
    }
    use std::cmp::Ordering::{Greater, Less};
    match operation {
        "max" => values
            .iter()
            .fold(values[0], |best, v| {
                if compare(v, best) == Some(Greater) {
                    v
                } else {
                    best
                }
            })
            .clone(),
        "min" => values
            .iter()
            .fold(values[0], |best, v| {
                if compare(v, best) == Some(Less) {
                    v
                } else {
                    best
                }
            })
            .clone(),
        _ => {
            let sum: f64 = values.iter().filter_map(|v| v.as_f64()).sum();
            let result = if operation == "avg" {
                sum / values.len() as f64
            } else {
                sum
            };
            if result.fract() == 0.0 && operation == "sum" {
                Json::Int(result as i64)
            } else {
                Json::Float(result)
            }
        }
    }
}

fn order_items(order: &Json) -> Vec<Json> {
    match order {
        Json::Array(items) => items.clone(),
        Json::Null => Vec::new(),
        other => vec![other.clone()],
    }
}

fn flatten_order(item: &Json, path: &mut Vec<String>, specs: &mut Vec<(Vec<String>, String)>) {
    let Json::Object(pairs) = item else {
        return;
    };
    for (key, value) in pairs {
        path.push(key.clone());
        match value.as_str() {
            Some(direction) => specs.push((path.clone(), direction.to_string())),
            None => flatten_order(value, path, specs),
        }
        path.pop();
    }
}

/// Postgres's defaults: ascending puts nulls last and descending first.
fn order_values(left: &Json, right: &Json, direction: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering::{Equal, Greater, Less};
    let descending = direction.starts_with("desc");
    let nulls_first =
        direction.ends_with("nulls_first") || (descending && !direction.ends_with("nulls_last"));
    match (left.is_null(), right.is_null()) {
        (true, true) => Equal,
        (true, false) => {
            if nulls_first {
                Less
            } else {
                Greater
            }
        }
        (false, true) => {
            if nulls_first {
                Greater
            } else {
                Less
            }
        }
        _ => {
            let found = compare(left, right).unwrap_or(Equal);
            if descending {
                found.reverse()
            } else {
                found
            }
        }
    }
}
