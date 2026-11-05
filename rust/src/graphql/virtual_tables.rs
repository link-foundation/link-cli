//! Tables Deep derives from links with triggers and views. Here they are
//! computed from the links when asked, for as long as nothing is written:
//! values and `can` always, `tree` and `selectors` from the `Tree*` and
//! `Selector*` links, `mp` the same paths under its own column names.
//! `handlers` is empty, as Deep's handlers do not run here.

use std::collections::{BTreeMap, HashMap, HashSet};

use super::execute::{Exec, GqlResult};
use super::json::Json;
use super::store::{LinkRow, ValueRow};
use crate::triplets::TripletQuery;

/// Which `(parent, child)` edges a link of an included type makes, over its
/// `from`, itself and its `to`. These are the groups of include types Deep's
/// `isAllowSpread*` conditions test, in `1621815803572-materialized-path.ts`.
const SPREADS: [(&str, [&str; 4]); 4] = [
    (
        "from-link",
        [
            "TreeIncludeDown",
            "TreeIncludeIn",
            "TreeIncludeFromCurrent",
            "TreeIncludeFromCurrentTo",
        ],
    ),
    (
        "to-link",
        [
            "TreeIncludeUp",
            "TreeIncludeIn",
            "TreeIncludeToCurrent",
            "TreeIncludeToCurrentFrom",
        ],
    ),
    (
        "link-to",
        [
            "TreeIncludeDown",
            "TreeIncludeOut",
            "TreeIncludeCurrentTo",
            "TreeIncludeFromCurrentTo",
        ],
    ),
    (
        "link-from",
        [
            "TreeIncludeUp",
            "TreeIncludeOut",
            "TreeIncludeCurrentFrom",
            "TreeIncludeToCurrentFrom",
        ],
    ),
];

/// Every other include type: it only makes a link a member of its tree.
const MEMBERSHIP_ONLY: [&str; 1] = ["TreeIncludeNode"];

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
            "tree" => self.derived("tree", Self::tree_rows),
            "selectors" => self.derived("selectors", Self::selector_rows),
            "mp" => self.mp_rows(),
            "healthz" => vec![Json::object(vec![("status", Json::str("ok"))])],
            _ => Vec::new(),
        })
    }

    /// A table computed from the links, kept until the next write.
    fn derived(&mut self, table: &'static str, compute: fn(&mut Self) -> Vec<Json>) -> Vec<Json> {
        let generation = self.store.generation;
        if !self.store.in_mutation {
            if let Some((at, rows)) = self.store.derived.get(table) {
                if *at == generation {
                    return rows.clone();
                }
            }
        }
        let rows = compute(self);
        if !self.store.in_mutation {
            self.store.derived.insert(table, (generation, rows.clone()));
        }
        rows
    }

    /// The policy's grants as Deep's `can`: a rule on `Anything` stands as the
    /// id `0` rather than one row for each link it reaches.
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

    /// `tree`: every path from a root to a link, one row per link on it, as
    /// Deep's materialized path stores them. `typesTree` is built in: a link's
    /// parent is its type.
    fn tree_rows(&mut self) -> Vec<Json> {
        let links = self.all_links();
        let any = self.core_id("Any");
        let types_tree = self.core_id("typesTree");
        let mut spreads: HashMap<i64, Vec<&'static str>> = HashMap::new();
        for (spread, names) in SPREADS {
            for name in names {
                if let Some(id) = self.core_id(name) {
                    spreads.entry(id).or_default().push(spread);
                }
            }
        }
        for name in MEMBERSHIP_ONLY {
            if let Some(id) = self.core_id(name) {
                spreads.entry(id).or_default();
            }
        }
        // tree -> included type -> the edges its links make.
        let mut trees: BTreeMap<i64, HashMap<i64, HashSet<&'static str>>> = BTreeMap::new();
        for link in &links {
            if Some(link.from_id) == types_tree {
                continue;
            }
            if let Some(made) = spreads.get(&link.type_id) {
                trees
                    .entry(link.from_id)
                    .or_default()
                    .entry(link.to_id)
                    .or_default()
                    .extend(made.iter().copied());
            }
        }
        let mut rows = Vec::new();
        if let Some(tree) = types_tree {
            let parents = links
                .iter()
                .filter(|link| link.type_id != link.id && link.type_id != 0)
                .map(|link| (link.id, link.type_id))
                .fold(HashMap::<i64, Vec<i64>>::new(), |mut all, (id, parent)| {
                    all.entry(id).or_default().push(parent);
                    all
                });
            let nodes: Vec<i64> = links.iter().map(|link| link.id).collect();
            push_paths(&mut rows, tree, nodes, &parents);
        }
        for (tree, includes) in trees {
            let wanted = |type_id: i64| {
                includes.contains_key(&type_id) || any.is_some_and(|a| includes.contains_key(&a))
            };
            let members: HashSet<i64> = links
                .iter()
                .filter(|link| wanted(link.type_id))
                .map(|link| link.id)
                .collect();
            let mut parents: HashMap<i64, Vec<i64>> = HashMap::new();
            for link in links.iter().filter(|link| members.contains(&link.id)) {
                let made = includes.get(&link.type_id);
                for spread in made.into_iter().flatten() {
                    let (parent, child) = match *spread {
                        "from-link" => (link.from_id, link.id),
                        "to-link" => (link.to_id, link.id),
                        "link-to" => (link.id, link.to_id),
                        _ => (link.id, link.from_id),
                    };
                    if parent != child && members.contains(&parent) && members.contains(&child) {
                        let list = parents.entry(child).or_default();
                        if !list.contains(&parent) {
                            list.push(parent);
                        }
                    }
                }
            }
            let mut nodes: Vec<i64> = members.into_iter().collect();
            nodes.sort_unstable();
            push_paths(&mut rows, tree, nodes, &parents);
        }
        rows
    }

    /// `mp`: the rows of `tree`, as Deep's own table names their columns.
    fn mp_rows(&mut self) -> Vec<Json> {
        let renamed = [
            ("id", "id"),
            ("item_id", "link_id"),
            ("path_item_id", "parent_id"),
            ("path_item_depth", "depth"),
            ("root_id", "root_id"),
            ("position_id", "position_id"),
            ("group_id", "tree_id"),
        ];
        self.derived("tree", Self::tree_rows)
            .iter()
            .map(|row| {
                Json::object(
                    renamed
                        .iter()
                        .map(|(name, column)| {
                            (*name, row.get(column).cloned().unwrap_or(Json::Null))
                        })
                        .collect(),
                )
            })
            .collect()
    }

    /// `selectors`: what a `SelectorInclude` reaches down its tree.
    fn selector_rows(&mut self) -> Vec<Json> {
        let (Some(include), Some(tree_link)) = (
            self.core_id("SelectorInclude"),
            self.core_id("SelectorTree"),
        ) else {
            return Vec::new();
        };
        let filter = self.core_id("SelectorFilter");
        let links = self.all_links();
        let tree_rows = self.derived("tree", Self::tree_rows);
        let mut reached: HashMap<(i64, i64), Vec<i64>> = HashMap::new();
        let int = |row: &Json, key: &str| row.get(key).and_then(Json::as_i64).unwrap_or(0);
        for row in &tree_rows {
            reached
                .entry((int(row, "tree_id"), int(row, "parent_id")))
                .or_default()
                .push(int(row, "link_id"));
        }
        let mut trees_of: HashMap<i64, Vec<i64>> = HashMap::new();
        let mut query_of: HashMap<i64, i64> = HashMap::new();
        for link in &links {
            if link.type_id == tree_link {
                trees_of.entry(link.from_id).or_default().push(link.to_id);
            } else if Some(link.type_id) == filter {
                query_of.insert(link.from_id, link.to_id);
            }
        }
        let mut rows = Vec::new();
        for link in links.iter().filter(|link| link.type_id == include) {
            for tree in trees_of.get(&link.id).into_iter().flatten() {
                for item in reached.get(&(*tree, link.to_id)).into_iter().flatten() {
                    rows.push(Json::object(vec![
                        ("item_id", Json::Int(*item)),
                        ("selector_id", Json::Int(link.from_id)),
                        ("selector_include_id", Json::Int(link.id)),
                        (
                            "query_id",
                            Json::Int(query_of.get(&link.from_id).copied().unwrap_or(0)),
                        ),
                    ]));
                }
            }
        }
        rows
    }
}

/// The rows of every path to each of `nodes`. Deep refuses cycles when they are
/// written; here a link already on a path ends it.
fn push_paths(rows: &mut Vec<Json>, tree: i64, nodes: Vec<i64>, parents: &HashMap<i64, Vec<i64>>) {
    fn walk(
        node: i64,
        parents: &HashMap<i64, Vec<i64>>,
        path: &mut Vec<i64>,
        out: &mut Vec<Vec<i64>>,
    ) {
        path.push(node);
        let above: Vec<i64> = parents
            .get(&node)
            .into_iter()
            .flatten()
            .copied()
            .filter(|parent| !path.contains(parent))
            .collect();
        if above.is_empty() {
            out.push(path.iter().rev().copied().collect());
        }
        for parent in above {
            walk(parent, parents, path, out);
        }
        path.pop();
    }
    for node in nodes {
        let mut paths = Vec::new();
        walk(node, parents, &mut Vec::new(), &mut paths);
        for (position, path) in paths.iter().enumerate() {
            for (depth, parent) in path.iter().enumerate() {
                let id = rows.len() as i64 + 1;
                rows.push(Json::object(vec![
                    ("id", Json::Int(id)),
                    ("link_id", Json::Int(node)),
                    ("tree_id", Json::Int(tree)),
                    ("root_id", Json::Int(path[0])),
                    ("parent_id", Json::Int(*parent)),
                    ("depth", Json::Int(depth as i64)),
                    (
                        "position_id",
                        Json::String(format!("{tree}-{node}-{position}")),
                    ),
                    ("self", Json::Bool(*parent == node)),
                ]));
            }
        }
    }
}
