//! The links interface (`ILinks` in C#, [`doublets::Links`] in Rust) as LiNo
//! documents, so a store behind a [`LinksServer`](super::LinksServer) is used
//! exactly like a local one through [`RemoteLinks`](super::RemoteLinks).
//!
//! Every request is one top-level link named after the operation and holding
//! exactly one value. The substitution query language ignores that shape (a
//! query needs a restriction *and* a substitution), so an operation never
//! collides with a query sent to the same server.
//!
//! | Request                          | Reply                                  |
//! |----------------------------------|----------------------------------------|
//! | `(count: (index source target))` | `(count: N)`                           |
//! | `(each: (index source target))`  | one `(index: source target)` per match |
//! | `(create: (source target))`      | `() ((index: source target))`          |
//! | `(update: (index source target))`| `((index: s t)) ((index: source target))` |
//! | `(delete: index)`                | `((index: source target)) ()` per removed link |
//! | `(get-name: link)`               | `(name: 'text')`, or nothing           |
//! | `(set-name: (link 'text'))`      | `(link: N)`                            |
//! | `(get-by-name: 'text')`          | `(link: N)`, or nothing                |
//! | `(remove-name: link)`            | nothing                                |
//!
//! A restriction holds up to three parts, matched like the raw links
//! interface matches a query: none matches every link, `(index)` one link,
//! `(index value)` links of that index whose source or target is `value`,
//! and `(index source target)` matches part by part. `*` matches anything.
//! Every reference in a reply is a number; failures are `(error: 'message')`.

use super::error::{ProtocolError, ProtocolResult};
use super::mapping::LinoDocument;
use crate::link::Link;
use crate::named_type_links::NamedTypeLinks;
use links_notation::LiNo;

/// The wire spelling of "any value" in a restriction.
pub const ANY: &str = "*";

/// One part of a restriction: `None` matches any value.
pub type Part = Option<u32>;

/// One call of the links interface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinksOperation {
    /// Number of links matching the restriction.
    Count(Vec<Part>),
    /// Every link matching the restriction, ordered by index.
    Each(Vec<Part>),
    /// Creates a link.
    Create { source: u32, target: u32 },
    /// Points an existing link at a new source and target.
    Update {
        index: u32,
        source: u32,
        target: u32,
    },
    /// Deletes a link, and every link that still refers to it.
    Delete(u32),
    /// The name of a link.
    GetName(u32),
    /// Names a link.
    SetName(u32, String),
    /// The link with a name.
    GetByName(String),
    /// Removes the name of a link.
    RemoveName(u32),
}

/// A `(before, after)` change; a missing side is [`Link::null`].
pub type Change = (Link, Link);

impl LinksOperation {
    /// The request document for this operation.
    pub fn to_document(&self) -> LinoDocument {
        let (name, argument) = match self {
            Self::Count(restriction) => ("count", restriction_lino(restriction)),
            Self::Each(restriction) => ("each", restriction_lino(restriction)),
            Self::Create { source, target } => ("create", numbers_lino(&[*source, *target])),
            Self::Update {
                index,
                source,
                target,
            } => ("update", numbers_lino(&[*index, *source, *target])),
            Self::Delete(index) => ("delete", number(*index)),
            Self::GetName(index) => ("get-name", number(*index)),
            Self::SetName(index, name) => (
                "set-name",
                group(vec![number(*index), LiNo::Ref(name.clone())]),
            ),
            Self::GetByName(name) => ("get-by-name", LiNo::Ref(name.clone())),
            Self::RemoveName(index) => ("remove-name", number(*index)),
        };
        vec![named(name, vec![argument])]
    }

    /// Recognises an operation request; `Ok(None)` for any other document,
    /// such as a substitution query.
    pub fn from_document(document: &[LiNo<String>]) -> ProtocolResult<Option<Self>> {
        let [LiNo::Link {
            id: Some(name),
            values,
        }] = document
        else {
            return Ok(None);
        };
        let [argument] = values.as_slice() else {
            return Ok(None);
        };
        let operation = match name.as_str() {
            "count" => Self::Count(parse_restriction(argument)?),
            "each" => Self::Each(parse_restriction(argument)?),
            "create" => {
                let [source, target] = parse_numbers::<2>(argument)?;
                Self::Create { source, target }
            }
            "update" => {
                let [index, source, target] = parse_numbers::<3>(argument)?;
                Self::Update {
                    index,
                    source,
                    target,
                }
            }
            "delete" => Self::Delete(parse_number(argument)?),
            "get-name" => Self::GetName(parse_number(argument)?),
            "set-name" => match parts(argument) {
                [index, LiNo::Ref(name)] => Self::SetName(parse_number(index)?, name.clone()),
                _ => return Err(malformed_argument("set-name", argument)),
            },
            "get-by-name" => match argument {
                LiNo::Ref(name) => Self::GetByName(name.clone()),
                _ => return Err(malformed_argument("get-by-name", argument)),
            },
            "remove-name" => Self::RemoveName(parse_number(argument)?),
            _ => return Ok(None),
        };
        Ok(Some(operation))
    }

    /// Runs the operation against `storage` and returns the reply document.
    pub fn execute<S: NamedTypeLinks + ?Sized>(
        &self,
        storage: &mut S,
    ) -> anyhow::Result<LinoDocument> {
        Ok(match self {
            Self::Count(restriction) => {
                let count = matching(storage, restriction).len();
                vec![named("count", vec![LiNo::Ref(count.to_string())])]
            }
            Self::Each(restriction) => matching(storage, restriction)
                .iter()
                .map(link_lino)
                .collect(),
            Self::Create { source, target } => {
                let index = storage.create(*source, *target);
                let created = storage
                    .get_link(index)
                    .unwrap_or_else(|| Link::new(index, *source, *target));
                storage.save()?;
                changes_document(&[(Link::null(), created)])
            }
            Self::Update {
                index,
                source,
                target,
            } => {
                let before = storage.update(*index, *source, *target)?;
                let after = storage
                    .get_link(*index)
                    .unwrap_or_else(|| Link::new(*index, *source, *target));
                storage.save()?;
                changes_document(&[(before, after)])
            }
            Self::Delete(index) => {
                let mut changes = Vec::new();
                storage
                    .delete_observed(*index, &mut |before, after| changes.push((before, after)))?;
                storage.save()?;
                changes_document(&changes)
            }
            Self::GetName(index) => match storage.get_name(*index)? {
                Some(name) => vec![named("name", vec![LiNo::Ref(name)])],
                None => Vec::new(),
            },
            Self::SetName(index, name) => {
                let link = storage.set_name(*index, name)?;
                storage.save()?;
                link_reply(Some(link))
            }
            Self::GetByName(name) => link_reply(storage.get_by_name(name)?),
            Self::RemoveName(index) => {
                storage.remove_name(*index)?;
                storage.save()?;
                Vec::new()
            }
        })
    }
}

/// Whether `link` matches `restriction`, with the shapes of the raw links
/// interface: see the [module documentation](self).
pub fn matches(link: &Link, restriction: &[Part]) -> bool {
    let is = |part: Part, value: u32| part.is_none_or(|part| part == value);
    match *restriction {
        [] => true,
        [index] => is(index, link.index),
        [index, value] => {
            is(index, link.index) && (is(value, link.source) || is(value, link.target))
        }
        [index, source, target] => {
            is(index, link.index) && is(source, link.source) && is(target, link.target)
        }
        _ => false,
    }
}

fn matching<S: NamedTypeLinks + ?Sized>(storage: &mut S, restriction: &[Part]) -> Vec<Link> {
    let mut links = match restriction.first() {
        Some(Some(index)) => storage.get_link(*index).into_iter().collect(),
        _ => storage.all_links(),
    };
    links.retain(|link| matches(link, restriction));
    links.sort_by_key(|link| link.index);
    links
}

/// The `count` of a `(count: N)` reply.
pub fn parse_count(document: &[LiNo<String>]) -> ProtocolResult<u32> {
    match document {
        [LiNo::Link {
            id: Some(id),
            values,
        }] if id == "count" => match values.as_slice() {
            [count] => parse_number(count),
            _ => Err(malformed_reply("count", document)),
        },
        _ => Err(malformed_reply("count", document)),
    }
}

/// The links of an `each` reply.
pub fn parse_links(document: &[LiNo<String>]) -> ProtocolResult<Vec<Link>> {
    document.iter().map(parse_link).collect()
}

/// The `(before) (after)` pairs of a `create`, `update` or `delete` reply.
pub fn parse_changes(document: &[LiNo<String>]) -> ProtocolResult<Vec<Change>> {
    document
        .iter()
        .map(|change| match parts(change) {
            [before, after] => Ok((parse_change_side(before)?, parse_change_side(after)?)),
            _ => Err(malformed_reply("change", document)),
        })
        .collect()
}

/// The name of a `get-name` reply.
pub fn parse_name(document: &[LiNo<String>]) -> ProtocolResult<Option<String>> {
    match document {
        [] => Ok(None),
        [LiNo::Link {
            id: Some(id),
            values,
        }] if id == "name" => match values.as_slice() {
            [LiNo::Ref(name)] => Ok(Some(name.clone())),
            _ => Err(malformed_reply("name", document)),
        },
        _ => Err(malformed_reply("name", document)),
    }
}

/// The link of a `set-name` or `get-by-name` reply.
pub fn parse_link_reply(document: &[LiNo<String>]) -> ProtocolResult<Option<u32>> {
    match document {
        [] => Ok(None),
        [LiNo::Link {
            id: Some(id),
            values,
        }] if id == "link" => match values.as_slice() {
            [link] => parse_number(link).map(Some),
            _ => Err(malformed_reply("link", document)),
        },
        _ => Err(malformed_reply("link", document)),
    }
}

/// A reply listing `changes`, one `(before) (after)` line each.
pub fn changes_document(changes: &[Change]) -> LinoDocument {
    changes
        .iter()
        .map(|(before, after)| group(vec![change_side(before), change_side(after)]))
        .collect()
}

/// `(index: source target)` with plain numbers.
pub fn link_lino(link: &Link) -> LiNo<String> {
    named(
        &link.index.to_string(),
        vec![number(link.source), number(link.target)],
    )
}

fn link_reply(link: Option<u32>) -> LinoDocument {
    link.map(|link| vec![named("link", vec![number(link)])])
        .unwrap_or_default()
}

fn change_side(link: &Link) -> LiNo<String> {
    if link.is_null() {
        group(Vec::new())
    } else {
        group(vec![link_lino(link)])
    }
}

fn parse_change_side(side: &LiNo<String>) -> ProtocolResult<Link> {
    match side {
        LiNo::Link { id: None, values } if values.is_empty() => Ok(Link::null()),
        LiNo::Link { id: None, values } if values.len() == 1 => parse_link(&values[0]),
        // `((index: source target))` loses its wrapper when the side is the
        // single named link itself.
        link @ LiNo::Link { id: Some(_), .. } => parse_link(link),
        _ => Err(ProtocolError::malformed(format!(
            "expected a change side, found {side:?}"
        ))),
    }
}

fn parse_link(link: &LiNo<String>) -> ProtocolResult<Link> {
    match link {
        LiNo::Link {
            id: Some(index),
            values,
        } => match values.as_slice() {
            [source, target] => Ok(Link::new(
                parse_text_number(index)?,
                parse_number(source)?,
                parse_number(target)?,
            )),
            _ => Err(ProtocolError::malformed(format!(
                "expected (index: source target), found {link:?}"
            ))),
        },
        _ => Err(ProtocolError::malformed(format!(
            "expected (index: source target), found {link:?}"
        ))),
    }
}

fn restriction_lino(restriction: &[Part]) -> LiNo<String> {
    group(
        restriction
            .iter()
            .map(|part| part.map_or_else(|| LiNo::Ref(ANY.to_string()), number))
            .collect(),
    )
}

fn parse_restriction(argument: &LiNo<String>) -> ProtocolResult<Vec<Part>> {
    let parts = parts(argument);
    if parts.len() > 3 {
        return Err(malformed_argument("restriction", argument));
    }
    parts
        .iter()
        .map(|part| match part {
            LiNo::Ref(text) if text == ANY => Ok(None),
            part => parse_number(part).map(Some),
        })
        .collect()
}

fn numbers_lino(numbers: &[u32]) -> LiNo<String> {
    group(numbers.iter().copied().map(number).collect())
}

fn parse_numbers<const N: usize>(argument: &LiNo<String>) -> ProtocolResult<[u32; N]> {
    let numbers = parts(argument)
        .iter()
        .map(parse_number)
        .collect::<ProtocolResult<Vec<_>>>()?;
    numbers
        .try_into()
        .map_err(|_| malformed_argument(&format!("{N} numbers"), argument))
}

/// The values of an unnamed group; a lone reference is a group of one, since
/// the canonical document model unwraps `(x)` to `x`.
fn parts(argument: &LiNo<String>) -> &[LiNo<String>] {
    match argument {
        LiNo::Link { id: None, values } => values,
        reference => std::slice::from_ref(reference),
    }
}

fn parse_number(value: &LiNo<String>) -> ProtocolResult<u32> {
    match value {
        LiNo::Ref(text) => parse_text_number(text),
        _ => Err(ProtocolError::malformed(format!(
            "expected a number, found {value:?}"
        ))),
    }
}

fn parse_text_number(text: &str) -> ProtocolResult<u32> {
    text.parse()
        .map_err(|_| ProtocolError::malformed(format!("expected a number, found '{text}'")))
}

fn number(value: u32) -> LiNo<String> {
    LiNo::Ref(value.to_string())
}

fn named(id: &str, values: Vec<LiNo<String>>) -> LiNo<String> {
    LiNo::Link {
        id: Some(id.to_string()),
        values,
    }
}

fn group(values: Vec<LiNo<String>>) -> LiNo<String> {
    LiNo::Link { id: None, values }
}

fn malformed_argument(expected: &str, argument: &LiNo<String>) -> ProtocolError {
    ProtocolError::malformed(format!("expected {expected}, found {argument:?}"))
}

fn malformed_reply(expected: &str, document: &[LiNo<String>]) -> ProtocolError {
    ProtocolError::malformed(format!("expected a {expected} reply, found {document:?}"))
}
