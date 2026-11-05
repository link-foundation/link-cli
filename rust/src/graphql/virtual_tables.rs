//! Tables Deep derives from links with triggers and views. Here they are
//! computed from the links when asked, for as long as nothing is written:
//! values and `can` always, `tree` and `selectors` from the `Tree*` and
//! `Selector*` links. `handlers` and `mp` are empty, as Deep's handlers do
//! not run here, and `tree` is what is read instead of `mp`.

use std::collections::{HashMap, HashSet};

use super::execute::{Exec, GqlResult};
use super::json::Json;
use super::store::{LinkRow, ValueRow};
use crate::triplets::TripletQuery;

/// How a link of an included type places what it touches in a tree, as
/// `(child, parent)` over its `from`, itself and its `to`.
#[derive(Clone, Copy, PartialEq)]
enum End {
    From,
    Link,
    To,
}

const RULES: [(&str, &[(End, End)]); 10] = [
    ("TreeIncludeNode", &[]),
    (
        "TreeIncludeDown",
        &[(End::Link, End::From), (End::To, End::Link)],
    ),
    (
        "TreeIncludeUp",
        &[(End::Link, End::To), (End::From, End::Link)],
    ),
    ("TreeIncludeFromCurrent", &[(End::Link, End::From)]),
    ("TreeIncludeToCurrent", &[(End::Link, End::To)]),
    ("TreeIncludeCurrentFrom", &[(End::From, End::Link)]),
    ("TreeIncludeCurrentTo", &[(End::To, End::Link)]),
    (
        "TreeIncludeFromCurrentTo",
        &[(End::Link, End::From), (End::To, End::Link)],
    ),
    (
        "TreeIncludeToCurrentFrom",
        &[(End::Link, End::To), (End::From, End::Link)],
    ),
    ("TreeIncludeOut", &[(End::Link, End::From)]),
];

impl Exec<'_, '_> {
    pub fn value_json(row: u32, value: &ValueRow) -> Json {
        Json::object(vec![
            ("id", Json::Int(i64::from(row))),
            ("link_id", Json::Int(value.owner)),
            ("value", value.value.clone()),
        ])
    }

    /// The row of a triplet below permissions, for the server's own lookups.
    pub fn raw_row(&mut self, triplet: u32) -> Option<LinkRow> {
        let admin = std::mem::replace(&mut self.store.admin, true);
        let found = self.store.view(0, |view| view.get_triplet(triplet));
        self.store.admin = admin;
        self.store.row_of(&found?)
    }

    fn all_links(&mut self) -> Vec<LinkRow> {
        let admin = std::mem::replace(&mut self.store.admin, true);
        let all = self
            .store
            .view(0, |view| view.each_triplet(TripletQuery::any()));
        self.store.admin = admin;
        all.iter()
            .filter_map(|triplet| self.store.row_of(triplet))
            .collect()
    }

    /// Every row of a table that is not `links`, as the subject sees them.
    pub fn table_rows(&mut self, table: &str) -> GqlResult<Vec<Json>> {
        Ok(match table {
            "strings" | "numbers" | "objects" => {
                let kind = super::store::ValueKind::of_table(table);
                let rows: Vec<(u32, ValueRow)> = self
                    .store
                    .values()
                    .filter(|(_, value)| Some(value.kind) == kind)
                    .map(|(row, value)| (row, value.clone()))
                    .collect();
                // A value is seen with its owner.
                let subject = self.subject;
                let policy = self.store.has_policy();
                let mut out = Vec::with_capacity(rows.len());
                for (row, value) in rows {
                    let seen = !policy || {
                        let address = self.store.triplet_of(value.owner);
                        address.is_some_and(|address| {
                            self.store
                                .view(subject, |view| view.get_triplet(address))
                                .is_some()
                        })
                    };
                    if seen {
                        out.push(Self::value_json(row, &value));
                    }
                }
                out
            }
            "can" => self.can_rows(),
            "tree" => self.tree_rows(),
            "selectors" => self.selector_rows(),
            "healthz" => vec![Json::object(vec![("status", Json::str("ok"))])],
            _ => Vec::new(),
        })
    }

    /// The policy's grants as Deep's `can`: a rule on `Anything` is one row for
    /// each link it reaches is not listed, but stands as the id `0`.
    fn can_rows(&mut self) -> Vec<Json> {
        let (select, update) = (self.core_id("AllowSelect"), self.core_id("AllowUpdate"));
        let mut rows = Vec::new();
        for (write, subject, object) in self.store.grants() {
            let action = if write { update } else { select };
            rows.push(Json::object(vec![
                ("rule_id", Json::Int(0)),
                ("action_id", action.map_or(Json::Int(0), Json::Int)),
                ("object_id", Json::Int(object.unwrap_or(0))),
                ("subject_id", Json::Int(subject.unwrap_or(0))),
            ]));
        }
        rows
    }

    /// Whether `link` is of `type_id`, `Any` taking every link.
    fn tree_rows(&mut self) -> Vec<Json> {
        let links = self.all_links();
        let any = self.core_id("Any");
        let mut rule_of: HashMap<i64, &'static [(End, End)]> = HashMap::new();
        for (name, rule) in RULES {
            if let Some(id) = self.core_id(name) {
                rule_of.insert(id, rule);
            }
        }
        let mut by_tree: HashMap<i64, Vec<(i64, &'static [(End, End)])>> = HashMap::new();
        for link in &links {
            if let Some(rule) = rule_of.get(&link.type_id) {
                by_tree
                    .entry(link.from_id)
                    .or_default()
                    .push((link.to_id, rule));
            }
        }
        let mut trees: Vec<_> = by_tree.into_iter().collect();
        trees.sort_by_key(|(tree, _)| *tree);
        let mut rows = Vec::new();
        for (tree, includes) in trees {
            let mut nodes = HashSet::new();
            let mut parents: HashMap<i64, Vec<i64>> = HashMap::new();
            for link in &links {
                for (included, rule) in &includes {
                    if *included != link.type_id && Some(*included) != any {
                        continue;
                    }
                    nodes.insert(link.id);
                    let end = |which: End| match which {
                        End::From => link.from_id,
                        End::Link => link.id,
                        End::To => link.to_id,
                    };
                    for (child, parent) in rule.iter() {
                        let (child, parent) = (end(*child), end(*parent));
                        if child != 0 && parent != 0 && child != parent {
                            parents.entry(child).or_default().push(parent);
                            nodes.insert(child);
                            nodes.insert(parent);
                        }
                    }
                }
            }
            let mut ordered: Vec<i64> = nodes.into_iter().collect();
            ordered.sort_unstable();
            for node in ordered {
                let mut seen = HashSet::from([node]);
                let mut level = vec![node];
                let mut climbed = vec![(node, 0)];
                let mut depth = 0;
                while !level.is_empty() {
                    depth += 1;
                    let mut next = Vec::new();
                    for at in &level {
                        for parent in parents.get(at).into_iter().flatten() {
                            if seen.insert(*parent) {
                                next.push(*parent);
                                climbed.push((*parent, depth));
                            }
                        }
                    }
                    level = next;
                }
                let root = climbed.last().map_or(node, |(top, _)| *top);
                let total = climbed.last().map_or(0, |(_, depth)| *depth);
                for (parent, up) in climbed {
                    let id = rows.len() as i64 + 1;
                    rows.push(Json::object(vec![
                        ("id", Json::Int(id)),
                        ("link_id", Json::Int(node)),
                        ("tree_id", Json::Int(tree)),
                        ("root_id", Json::Int(root)),
                        ("parent_id", Json::Int(parent)),
                        ("depth", Json::Int(total - up)),
                        ("position_id", Json::String(format!("{tree}-{root}"))),
                        ("self", Json::Bool(up == 0)),
                    ]));
                }
            }
        }
        rows
    }

    /// `selectors`: what a `SelectorInclude` reaches down its tree.
    fn selector_rows(&mut self) -> Vec<Json> {
        let (Some(include), Some(tree_link)) = (
            self.core_id("SelectorInclude"),
            self.core_id("SelectorTree"),
        ) else {
            return Vec::new();
        };
        let links = self.all_links();
        let tree_rows = self.tree_rows();
        let int = |row: &Json, key: &str| row.get(key).and_then(Json::as_i64).unwrap_or(0);
        let mut rows = Vec::new();
        for link in links.iter().filter(|link| link.type_id == include) {
            let trees = links
                .iter()
                .filter(|other| other.type_id == tree_link && other.from_id == link.id);
            for tree in trees {
                for row in tree_rows.iter().filter(|row| {
                    int(row, "tree_id") == tree.to_id && int(row, "parent_id") == link.to_id
                }) {
                    rows.push(Json::object(vec![
                        ("item_id", Json::Int(int(row, "link_id"))),
                        ("selector_id", Json::Int(link.from_id)),
                        ("selector_include_id", Json::Int(link.id)),
                        ("query_id", Json::Int(0)),
                    ]));
                }
            }
        }
        rows
    }
}
