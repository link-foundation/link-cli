//! The tables of the Deep schema: their relations, as Hasura's metadata
//! declares them, and how rows are compared.

use regex_lite::RegexBuilder;

use super::json::Json;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Relation {
    pub local: &'static str,
    pub target: &'static str,
    pub remote: &'static str,
    /// An array relation yields many rows, an object relation at most one.
    pub many: bool,
}

const fn one(local: &'static str, target: &'static str, remote: &'static str) -> Relation {
    Relation {
        local,
        target,
        remote,
        many: false,
    }
}

const fn many(local: &'static str, target: &'static str, remote: &'static str) -> Relation {
    Relation {
        local,
        target,
        remote,
        many: true,
    }
}

pub const TABLES: [&str; 10] = [
    "links",
    "strings",
    "numbers",
    "objects",
    "can",
    "tree",
    "selectors",
    "handlers",
    "mp",
    "healthz",
];

pub fn relation(table: &str, name: &str) -> Option<Relation> {
    Some(match (table, name) {
        ("links", "from") => one("from_id", "links", "id"),
        ("links", "to") => one("to_id", "links", "id"),
        ("links", "type") => one("type_id", "links", "id"),
        ("links", "string") => one("id", "strings", "link_id"),
        ("links", "number") => one("id", "numbers", "link_id"),
        ("links", "object") => one("id", "objects", "link_id"),
        ("links", "in") => many("id", "links", "to_id"),
        ("links", "out") => many("id", "links", "from_id"),
        ("links", "typed") => many("id", "links", "type_id"),
        ("links", "can_action") => many("id", "can", "action_id"),
        ("links", "can_object") => many("id", "can", "object_id"),
        ("links", "can_rule") => many("id", "can", "rule_id"),
        ("links", "can_subject") => many("id", "can", "subject_id"),
        ("links", "down") => many("id", "tree", "parent_id"),
        ("links", "up") => many("id", "tree", "link_id"),
        ("links", "tree") => many("id", "tree", "tree_id"),
        ("links", "root") => many("id", "tree", "root_id"),
        ("links", "selected") => many("id", "selectors", "selector_id"),
        ("links", "selectors") => many("id", "selectors", "item_id"),
        ("links", "_by_group") => many("id", "mp", "group_id"),
        ("links", "_by_item") => many("id", "mp", "item_id"),
        ("links", "_by_path_item") => many("id", "mp", "path_item_id"),
        ("links", "_by_root") => many("id", "mp", "root_id"),
        ("strings" | "numbers" | "objects", "link") => one("link_id", "links", "id"),
        ("can", "action") => one("action_id", "links", "id"),
        ("can", "object") => one("object_id", "links", "id"),
        ("can", "rule") => one("rule_id", "links", "id"),
        ("can", "subject") => one("subject_id", "links", "id"),
        ("tree", "link") => one("link_id", "links", "id"),
        ("tree", "parent") => one("parent_id", "links", "id"),
        ("tree", "root") => one("root_id", "links", "id"),
        ("tree", "tree" | "by_tree") => one("tree_id", "links", "id"),
        ("tree", "by_link") => many("link_id", "tree", "link_id"),
        ("tree", "by_parent") => many("parent_id", "tree", "parent_id"),
        ("tree", "by_position") => many("position_id", "tree", "position_id"),
        ("tree", "by_root") => many("root_id", "tree", "root_id"),
        ("selectors", "item") => one("item_id", "links", "id"),
        ("selectors", "selector") => one("selector_id", "links", "id"),
        ("selectors", "query") => many("query_id", "links", "id"),
        ("handlers", "dist") => one("dist_id", "links", "id"),
        ("handlers", "execution_provider") => one("execution_provider_id", "links", "id"),
        ("handlers", "handler") => one("handler_id", "links", "id"),
        ("handlers", "isolation_provider") => one("isolation_provider_id", "links", "id"),
        ("handlers", "src") => one("src_id", "links", "id"),
        _ => return None,
    })
}

pub fn columns(table: &str) -> &'static [&'static str] {
    match table {
        "links" => &["id", "type_id", "from_id", "to_id"],
        "strings" | "numbers" | "objects" => &["id", "link_id", "value"],
        "can" => &["rule_id", "action_id", "object_id", "subject_id"],
        "tree" => &[
            "id",
            "link_id",
            "tree_id",
            "root_id",
            "parent_id",
            "depth",
            "position_id",
        ],
        "selectors" => &["item_id", "selector_id", "selector_include_id", "query_id"],
        "handlers" => &[
            "dist_id",
            "src_id",
            "handler_id",
            "isolation_provider_id",
            "execution_provider_id",
        ],
        "mp" => &[
            "id",
            "item_id",
            "path_item_id",
            "path_item_depth",
            "root_id",
            "position_id",
            "group_id",
        ],
        "healthz" => &["status"],
        _ => &[],
    }
}

/// Numbers compare as numbers, whatever way they were written.
pub fn compare(left: &Json, right: &Json) -> Option<std::cmp::Ordering> {
    match (left, right) {
        (Json::String(left), Json::String(right)) => Some(left.cmp(right)),
        (Json::Bool(left), Json::Bool(right)) => Some(left.cmp(right)),
        _ => left.as_f64()?.partial_cmp(&right.as_f64()?),
    }
}

/// SQL's `LIKE`: `%` is any run and `_` any one character.
fn like(text: &str, pattern: &str, insensitive: bool) -> bool {
    let fold = |value: &str| -> Vec<char> {
        if insensitive {
            value.to_lowercase().chars().collect()
        } else {
            value.chars().collect()
        }
    };
    let (text, pattern) = (fold(text), fold(pattern));
    let (mut at, mut position) = (0, 0);
    let mut retry = None;
    while at < text.len() {
        match pattern.get(position) {
            Some('%') => {
                retry = Some((at, position));
                position += 1;
            }
            Some(&expected) if expected == '_' || expected == text[at] => {
                at += 1;
                position += 1;
            }
            _ => match retry {
                Some((from, star)) => {
                    at = from + 1;
                    retry = Some((from + 1, star));
                    position = star + 1;
                }
                None => return false,
            },
        }
    }
    pattern[position..]
        .iter()
        .all(|&character| character == '%')
}

fn regex(text: &str, pattern: &str, insensitive: bool) -> bool {
    RegexBuilder::new(pattern)
        .case_insensitive(insensitive)
        .build()
        .map(|regex| regex.is_match(text))
        .unwrap_or(false)
}

fn contains(outer: &Json, inner: &Json) -> bool {
    match (outer, inner) {
        (Json::Object(_), Json::Object(pairs)) => pairs
            .iter()
            .all(|(key, value)| outer.get(key).is_some_and(|found| contains(found, value))),
        (Json::Array(items), Json::Array(wanted)) => wanted
            .iter()
            .all(|item| items.iter().any(|found| contains(found, item))),
        (left, right) => compare(left, right) == Some(std::cmp::Ordering::Equal) || left == right,
    }
}

/// Whether a column's value passes `{_op: argument}`; SQL's null compares
/// false to everything but `_is_null`.
pub fn passes(value: &Json, operator: &str, argument: &Json) -> Result<bool, String> {
    use std::cmp::Ordering::{Equal, Greater, Less};
    if operator == "_is_null" {
        return Ok(value.is_null() == matches!(argument, Json::Bool(true)));
    }
    if value.is_null() {
        return Ok(false);
    }
    let text = value.as_str().unwrap_or("");
    let pattern = argument.as_str().unwrap_or("");
    Ok(match operator {
        "_eq" => compare(value, argument) == Some(Equal) || value == argument,
        "_neq" => {
            compare(value, argument) != Some(Equal) && value != argument && !argument.is_null()
        }
        "_gt" => compare(value, argument) == Some(Greater),
        "_gte" => matches!(compare(value, argument), Some(Greater | Equal)),
        "_lt" => compare(value, argument) == Some(Less),
        "_lte" => matches!(compare(value, argument), Some(Less | Equal)),
        "_in" => argument
            .as_list()
            .iter()
            .any(|item| compare(value, item) == Some(Equal)),
        "_nin" => !argument
            .as_list()
            .iter()
            .any(|item| compare(value, item) == Some(Equal)),
        "_like" => like(text, pattern, false),
        "_nlike" => !like(text, pattern, false),
        "_ilike" => like(text, pattern, true),
        "_nilike" => !like(text, pattern, true),
        "_regex" => regex(text, pattern, false),
        "_nregex" => !regex(text, pattern, false),
        "_iregex" => regex(text, pattern, true),
        "_niregex" => !regex(text, pattern, true),
        "_contains" => contains(value, argument),
        "_contained_in" => contains(argument, value),
        "_has_key" => value.get(pattern).is_some(),
        other => {
            return Err(format!(
                "field '{other}' not found in type: 'comparison_exp'"
            ))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn like_handles_wildcards_and_case() {
        assert!(like("Hello", "H%o", false));
        assert!(like("Hello", "_ello", false));
        assert!(!like("Hello", "h%", false));
        assert!(like("Hello", "h%", true));
    }

    #[test]
    fn null_passes_only_is_null() {
        assert!(passes(&Json::Null, "_is_null", &Json::Bool(true)).unwrap());
        assert!(!passes(&Json::Null, "_neq", &Json::Int(1)).unwrap());
    }
}
