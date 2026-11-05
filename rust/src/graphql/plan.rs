//! Hasura's `links_bool_exp` as substitution queries.
//!
//! The part of a `where` that is a conjunction of equalities and relations
//! becomes one restriction over joined variables:
//! `{from: {type_id: {_eq: 5}}}` is `(($i: * $v1 *) ($v1: 5 * *))`. `_in` and
//! `_or` are unions of such queries, `_and` their intersection and `_not` what
//! the rest leaves. What a restriction cannot say (`_gt`, `_ilike`, values) is
//! left to the executor, which checks every candidate the plan found.

use super::json::Json;

/// Which part of a triplet an id stands in: type `0` is a triplet of its own,
/// because a triplet's type cannot be null.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    Index,
    Type,
    Other,
}

/// How a plan gets the rows of `links` it stands for.
#[derive(Debug, Clone, PartialEq)]
pub enum Plan {
    All,
    Empty,
    /// The values of `$i` over this restriction, as the query text.
    Match(String),
    /// Deep ids the `where` names outright.
    Ids(Vec<i64>),
    Union(Vec<Plan>),
    Intersect(Vec<Plan>),
    /// Every row but these.
    Minus(Box<Plan>),
}

/// A plan, and whether it says all the `where` does.
#[derive(Debug, Clone, PartialEq)]
pub struct Compiled {
    pub plan: Plan,
    pub exact: bool,
}

type Mapping<'a> = &'a dyn Fn(Position, i64) -> Option<u32>;

/// One triplet pattern under construction; a part is `None` while it is `*`.
#[derive(Default)]
struct Node {
    index: String,
    parts: [Option<String>; 3],
    impossible: bool,
}

impl Node {
    fn render(&self) -> String {
        let part = |slot: usize| self.parts[slot].as_deref().unwrap_or("*");
        format!("({}: {} {} {})", self.index, part(0), part(1), part(2))
    }

    fn set(&mut self, slot: usize, value: String) {
        match &self.parts[slot] {
            Some(found) if *found != value => self.impossible = true,
            _ => self.parts[slot] = Some(value),
        }
    }

    fn constraints(&self) -> usize {
        self.parts
            .iter()
            .flatten()
            .filter(|part| !part.starts_with('$'))
            .count()
    }
}

fn slot_of(column: &str) -> Option<usize> {
    match column {
        "type_id" => Some(0),
        "from_id" => Some(1),
        "to_id" => Some(2),
        _ => None,
    }
}

/// `from: {...}`: the column of this link holding the related link.
fn forward_slot(relation: &str) -> Option<usize> {
    match relation {
        "type" => Some(0),
        "from" => Some(1),
        "to" => Some(2),
        _ => None,
    }
}

/// `in: {...}`: the column of the related link holding this link.
fn backward_slot(relation: &str) -> Option<usize> {
    match relation {
        "typed" => Some(0),
        "out" => Some(1),
        "in" => Some(2),
        _ => None,
    }
}

fn single_eq(condition: &Json) -> Option<i64> {
    match condition {
        Json::Object(pairs) if pairs.len() == 1 && pairs[0].0 == "_eq" => pairs[0].1.as_i64(),
        _ => None,
    }
}

struct Builder<'a> {
    mapping: Mapping<'a>,
    nodes: Vec<Node>,
    next_variable: usize,
}

impl Builder<'_> {
    fn variable(&mut self) -> String {
        self.next_variable += 1;
        format!("$v{}", self.next_variable)
    }

    fn address(&self, position: Position, id: i64) -> Option<String> {
        if id == 0 && position == Position::Other {
            return Some("0".to_string());
        }
        (self.mapping)(position, id).map(|address| address.to_string())
    }

    /// Whether `where` is wholly a conjunction this restriction can say. When
    /// it is not, nothing is kept: the caller looks at it another way.
    fn pure(&mut self, node: usize, condition: &Json) -> bool {
        let mark = self.nodes.len();
        let saved = self.nodes[node].parts.clone();
        let Json::Object(pairs) = condition else {
            return false;
        };
        let pure =
            !pairs.is_empty() && pairs.iter().all(|(key, value)| self.term(node, key, value));
        if !pure {
            self.nodes.truncate(mark);
            self.nodes[node].parts = saved;
        }
        pure
    }

    /// Adds the term to `node`; false when the restriction cannot say it.
    fn term(&mut self, node: usize, key: &str, value: &Json) -> bool {
        if let Some(slot) = slot_of(key) {
            let Some(id) = single_eq(value) else {
                return false;
            };
            let position = if slot == 0 {
                Position::Type
            } else {
                Position::Other
            };
            match self.address(position, id) {
                Some(address) => self.nodes[node].set(slot, address),
                None => self.nodes[node].impossible = true,
            }
            return true;
        }
        if let Some(slot) = forward_slot(key) {
            let variable = self.variable();
            let child = self.nodes.len();
            self.nodes.push(Node {
                index: variable.clone(),
                ..Node::default()
            });
            if self.pure(child, value) {
                self.nodes[node].set(slot, variable);
                return true;
            }
            self.nodes.truncate(child);
            return false;
        }
        if let Some(slot) = backward_slot(key) {
            let variable = self.variable();
            let child = self.nodes.len();
            let parent = self.nodes[node].index.clone();
            let mut pattern = Node {
                index: variable,
                ..Node::default()
            };
            pattern.parts[slot] = Some(parent);
            self.nodes.push(pattern);
            if self.pure(child, value) {
                return true;
            }
            self.nodes.truncate(child);
            return false;
        }
        if key == "_and" {
            return value.as_list().iter().all(|item| match item {
                Json::Object(pairs) => pairs.iter().all(|(key, value)| self.term(node, key, value)),
                _ => false,
            });
        }
        false
    }
}

/// Plans a `where` over `links`.
pub fn compile(condition: &Json, mapping: Mapping) -> Compiled {
    let Json::Object(pairs) = condition else {
        return Compiled {
            plan: Plan::All,
            exact: true,
        };
    };
    let mut builder = Builder {
        mapping,
        nodes: vec![Node {
            index: "$i".to_string(),
            ..Node::default()
        }],
        next_variable: 0,
    };
    let mut plans = Vec::new();
    let mut exact = true;
    for (key, value) in pairs {
        if key == "id" {
            if let Some(ids) = id_list(value) {
                plans.push(Plan::Ids(ids));
                continue;
            }
        }
        let mark = builder.nodes.len();
        let saved = builder.nodes[0].parts.clone();
        if builder.term(0, key, value) {
            continue;
        }
        builder.nodes.truncate(mark);
        builder.nodes[0].parts = saved;
        let (plan, whole) = other(key, value, mapping);
        if let Some(plan) = plan {
            plans.push(plan);
        }
        exact &= whole;
    }
    let constrained = builder.nodes.len() > 1 || builder.nodes[0].parts.iter().any(Option::is_some);
    if builder.nodes.iter().any(|node| node.impossible) {
        return Compiled {
            plan: Plan::Empty,
            exact: true,
        };
    }
    if constrained {
        let root = builder.nodes.remove(0);
        let mut children = std::mem::take(&mut builder.nodes);
        // The most constrained first: each next pattern starts from bindings.
        children.sort_by_key(|node| std::cmp::Reverse(node.constraints()));
        let ordered: Vec<Node> = if root.constraints() > 0 {
            std::iter::once(root).chain(children).collect()
        } else {
            children.into_iter().chain(std::iter::once(root)).collect()
        };
        let text = ordered
            .iter()
            .map(Node::render)
            .collect::<Vec<_>>()
            .join(" ");
        plans.insert(0, Plan::Match(format!("({text})")));
    }
    let plan = match plans.len() {
        0 => Plan::All,
        1 => plans.remove(0),
        _ => Plan::Intersect(plans),
    };
    Compiled { plan, exact }
}

fn id_list(condition: &Json) -> Option<Vec<i64>> {
    let Json::Object(pairs) = condition else {
        return None;
    };
    if pairs.len() != 1 {
        return None;
    }
    match pairs[0].0.as_str() {
        "_eq" => pairs[0].1.as_i64().map(|id| vec![id]),
        "_in" => match &pairs[0].1 {
            Json::Array(items) => items.iter().map(Json::as_i64).collect(),
            _ => None,
        },
        _ => None,
    }
}

/// A term outside the conjunction: a plan narrowing the rows, if there is one,
/// and whether that plan says all the term does.
fn other(key: &str, value: &Json, mapping: Mapping) -> (Option<Plan>, bool) {
    match key {
        "_or" => {
            let items: Vec<Compiled> = value
                .as_list()
                .iter()
                .map(|item| compile(item, mapping))
                .collect();
            if items.is_empty() {
                return (None, false);
            }
            let exact = items.iter().all(|item| item.exact);
            (
                Some(Plan::Union(
                    items.into_iter().map(|item| item.plan).collect(),
                )),
                exact,
            )
        }
        "_and" => {
            let items: Vec<Compiled> = value
                .as_list()
                .iter()
                .map(|item| compile(item, mapping))
                .collect();
            let exact = items.iter().all(|item| item.exact);
            (
                Some(Plan::Intersect(
                    items.into_iter().map(|item| item.plan).collect(),
                )),
                exact,
            )
        }
        "_not" => {
            let inner = compile(value, mapping);
            if inner.exact {
                (Some(Plan::Minus(Box::new(inner.plan))), true)
            } else {
                (None, false)
            }
        }
        column => {
            let Some(slot) = slot_of(column) else {
                return (None, false);
            };
            let ids = match id_list(value) {
                Some(ids) => ids,
                None => return (None, false),
            };
            let branches = ids
                .into_iter()
                .map(|id| {
                    let mut single = vec![(
                        column.to_string(),
                        Json::object(vec![("_eq", Json::Int(id))]),
                    )];
                    let _ = slot;
                    compile(&Json::Object(std::mem::take(&mut single)), mapping).plan
                })
                .collect();
            (Some(Plan::Union(branches)), true)
        }
    }
}

/// A plan as text, for tests and `--trace`.
pub fn render(plan: &Plan) -> String {
    let list = |items: &[Plan]| items.iter().map(render).collect::<Vec<_>>().join(" ");
    match plan {
        Plan::All => "all".to_string(),
        Plan::Empty => "empty".to_string(),
        Plan::Match(text) => text.clone(),
        Plan::Ids(ids) => format!(
            "(ids {})",
            ids.iter().map(i64::to_string).collect::<Vec<_>>().join(" ")
        ),
        Plan::Union(items) => format!("(union {})", list(items)),
        Plan::Intersect(items) => format!("(intersect {})", list(items)),
        Plan::Minus(item) => format!("(minus {})", render(item)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deep id `n` is at address `n + 100`; type `0` at 99; id 404 is nowhere.
    fn map(position: Position, id: i64) -> Option<u32> {
        let _ = position;
        (id != 404)
            .then_some((id + 100) as u32)
            .map(|a| if id == 0 { 99 } else { a })
    }

    fn plan(text: &str) -> Compiled {
        compile(&Json::parse(text).unwrap(), &map)
    }

    fn matched(text: &str) -> Plan {
        Plan::Match(text.to_string())
    }

    /// The vectors both languages run: `where` to substitution text.
    #[test]
    fn shared_vectors() {
        let vectors = Json::parse(include_str!(
            "../../../docs/protocol/graphql/where-vectors.json"
        ))
        .unwrap();
        let Json::Array(vectors) = vectors else {
            panic!()
        };
        for vector in vectors {
            let found = compile(vector.get("where").unwrap(), &map);
            assert_eq!(
                render(&found.plan),
                vector.get("plan").unwrap().as_str().unwrap(),
                "{}",
                vector.get("where").unwrap().write()
            );
            assert_eq!(
                Some(found.exact),
                match vector.get("exact") {
                    Some(Json::Bool(exact)) => Some(*exact),
                    _ => None,
                }
            );
        }
    }

    #[test]
    fn equality_and_relations_join_variables() {
        assert_eq!(
            plan(r#"{"type_id":{"_eq":5}}"#).plan,
            matched("(($i: 105 * *))")
        );
        let joined = plan(r#"{"from":{"type_id":{"_eq":5}}}"#);
        assert!(joined.exact);
        assert_eq!(joined.plan, matched("(($v1: 105 * *) ($i: * $v1 *))"));
    }
}
