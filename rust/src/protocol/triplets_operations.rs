//! The triplets of a server in triplet mode (issue #152) as LiNo documents, so
//! a store behind a [`LinksServer`](super::LinksServer) is used exactly like a
//! local [`Triplets`] store through [`RemoteLinks`](super::RemoteLinks).
//!
//! `(delete: i)`, `(count: ())` and a substitution query read the same in both
//! worlds, so the shape of a request cannot tell a triplet from a doublet.
//! That is why triplets are a mode of the server,
//! [`ServerOptions::triplets`](super::ServerOptions::triplets) (`clink --serve
//! --triplets`): a server without it answers exactly as it always did, and one
//! with it reads every request as triplets.
//!
//! | Request                       | Reply                                   |
//! |-------------------------------|-----------------------------------------|
//! | `(count: (index type from to))` | `(count: N)`                          |
//! | `(each: (index type from to))`  | one `(index: type from to)` per match |
//! | `(create: (type from to))`    | `() ((index: type from to))`            |
//! | `(create: (0 0 0))`           | `() ((index: index 0 0))`, a point: a triplet that is its own type |
//! | `(update: (index type from to))` | `((index: t f t)) ((index: type from to))` |
//! | `(delete: index)`             | `((index: type from to)) ()` per removed triplet, `index` first |
//! | `(subscribe: (index type from to))` | `(subscription: N)`; later pushes of `(notify: N)` and its changes |
//!
//! `get-name`, `set-name`, `get-by-name`, `remove-name`, `unsubscribe` and
//! `auth` are the same as [without triplets](super::links_operations), and the
//! empty request lists every triplet. Any other document is a substitution
//! query over triplets ([`process_triplets_query`]); its reply names the
//! references that have a name, like a query over doublets.
//!
//! A restriction holds up to four parts, matched part by part: the parts it
//! leaves out match anything, as does `*`; an empty `from` or `to` is `0`.
//! The wrappers and pairs that store a triplet in doublets never appear.
//!
//! A reply or push in a binary format whose arity holds three references but
//! not the five of `(index: type from to)`, such as `2..3`, carries every
//! triplet as `(index: (type from to))`, which travels as one link of three
//! references; see [`pack_triplets`]. Every parser here reads both forms.

use super::error::{ProtocolError, ProtocolResult};
use super::links_operations::{
    group, malformed_argument, malformed_reply, named, notification_number, number, numbers_lino,
    parse_number, parse_numbers, parse_restriction_of, parse_text_number, parts, restriction_lino,
    LinksOperation, Part,
};
use super::protocols::MessageFormat;
use super::server::{error_document, reference_name};
use crate::triplets::{process_triplets_query, Triplet, TripletChange, TripletQuery, Triplets};
use links_notation::binary::{format_document, format_link, LinoDocument};
use links_notation::LiNo;

/// One call of the [`Triplets`] interface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TripletsOperation {
    /// Number of triplets matching the restriction.
    Count(Vec<Part>),
    /// Every triplet matching the restriction, ordered by index.
    Each(Vec<Part>),
    /// Creates a triplet; `(0 0 0)` creates a point, its own type.
    Create { type_id: u32, from: u32, to: u32 },
    /// Makes an existing triplet `(index: type from to)`, keeping its address.
    Update {
        index: u32,
        type_id: u32,
        from: u32,
        to: u32,
    },
    /// Deletes a triplet and every triplet that uses it.
    Delete(u32),
    /// Asks a [`LinksServer`](super::LinksServer) to push the changes other
    /// connections make to the triplets matching the restriction; only the
    /// server can run it, so [`execute`](Self::execute) on a store fails.
    Subscribe(Vec<Part>),
}

/// A change a [`LinksServer`](super::LinksServer) in triplet mode pushed to a
/// subscriber; a side that does not fit the restriction (or that the
/// subscriber may not read) is `None`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TripletsNotification {
    /// The number the `subscribe` reply gave.
    pub subscription: u32,
    /// The changes.
    pub changes: Vec<TripletChange>,
}

impl TripletsOperation {
    /// The request document for this operation.
    pub fn to_document(&self) -> LinoDocument {
        let (name, argument) = match self {
            Self::Count(restriction) => ("count", restriction_lino(restriction)),
            Self::Each(restriction) => ("each", restriction_lino(restriction)),
            Self::Create { type_id, from, to } => ("create", numbers_lino(&[*type_id, *from, *to])),
            Self::Update {
                index,
                type_id,
                from,
                to,
            } => ("update", numbers_lino(&[*index, *type_id, *from, *to])),
            Self::Delete(index) => ("delete", number(*index)),
            Self::Subscribe(restriction) => ("subscribe", restriction_lino(restriction)),
        };
        vec![named(name, vec![argument])]
    }

    /// Recognises a triplet operation; `Ok(None)` for any other document, such
    /// as a substitution query or a request about names.
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
            "count" => Self::Count(parse_restriction_of(argument, 4)?),
            "each" => Self::Each(parse_restriction_of(argument, 4)?),
            "create" => {
                let [type_id, from, to] = parse_numbers::<3>(argument)?;
                Self::Create { type_id, from, to }
            }
            "update" => {
                let [index, type_id, from, to] = parse_numbers::<4>(argument)?;
                Self::Update {
                    index,
                    type_id,
                    from,
                    to,
                }
            }
            "delete" => Self::Delete(parse_number(argument)?),
            "subscribe" => Self::Subscribe(parse_restriction_of(argument, 4)?),
            _ => return Ok(None),
        };
        Ok(Some(operation))
    }

    /// Runs the operation against `triplets` and returns the reply document.
    pub fn execute<T: Triplets + ?Sized>(&self, triplets: &mut T) -> anyhow::Result<LinoDocument> {
        Ok(match self {
            Self::Count(restriction) => {
                let count = matching(triplets, restriction).len();
                vec![named("count", vec![number(count as u32)])]
            }
            Self::Each(restriction) => matching(triplets, restriction)
                .iter()
                .map(triplet_lino)
                .collect(),
            Self::Create { type_id, from, to } => {
                let created = if (*type_id, *from, *to) == (0, 0, 0) {
                    let index = triplets.create_triplet_point()?;
                    Triplet::new(index, index, 0, 0)
                } else {
                    let index = triplets.create_triplet(*type_id, *from, *to)?;
                    Triplet::new(index, *type_id, *from, *to)
                };
                triplets.save()?;
                triplet_changes_document(&[TripletChange {
                    before: None,
                    after: Some(created),
                }])
            }
            Self::Update {
                index,
                type_id,
                from,
                to,
            } => {
                let before = triplets.update_triplet(*index, *type_id, *from, *to)?;
                triplets.save()?;
                triplet_changes_document(&[TripletChange {
                    before: Some(before),
                    after: Some(Triplet::new(*index, *type_id, *from, *to)),
                }])
            }
            Self::Delete(index) => {
                let deleted = triplets.delete_triplet(*index)?;
                triplets.save()?;
                let changes: Vec<TripletChange> = deleted
                    .into_iter()
                    .map(|triplet| TripletChange {
                        before: Some(triplet),
                        after: None,
                    })
                    .collect();
                triplet_changes_document(&changes)
            }
            Self::Subscribe(_) => {
                anyhow::bail!("subscriptions belong to a connection to a links server")
            }
        })
    }
}

/// The query a restriction asks for; the parts it leaves out match anything.
pub fn triplet_query(restriction: &[Part]) -> TripletQuery {
    let part = |at: usize| restriction.get(at).copied().flatten();
    TripletQuery::new(part(0), part(1), part(2), part(3))
}

/// The restriction of `query`, in the order of the wire.
pub fn restriction_of_query(query: TripletQuery) -> Vec<Part> {
    vec![query.index, query.type_id, query.from, query.to]
}

/// Whether `triplet` matches `restriction`, part by part.
pub fn matches_triplet(triplet: &Triplet, restriction: &[Part]) -> bool {
    restriction.len() <= 4 && triplet_query(restriction).matches(triplet)
}

fn matching<T: Triplets + ?Sized>(triplets: &mut T, restriction: &[Part]) -> Vec<Triplet> {
    triplets.each_triplet(triplet_query(restriction))
}

/// Answers one request of a server in triplet mode against `triplets`;
/// failures produce an [`error_document`].
///
/// The counterpart of [`execute_request`](super::execute_request); see the
/// [module documentation](self).
pub fn execute_triplets_request<T: Triplets + ?Sized>(
    triplets: &mut T,
    document: &[LiNo<String>],
) -> LinoDocument {
    execute_observed_triplets_request(triplets, document, &mut Vec::new())
}

/// Like [`execute_triplets_request`], adding every triplet the request
/// changed to `changed`, with plain numbers.
pub(super) fn execute_observed_triplets_request<T: Triplets + ?Sized>(
    triplets: &mut T,
    document: &[LiNo<String>],
    changed: &mut Vec<TripletChange>,
) -> LinoDocument {
    match try_execute_triplets_request(triplets, document, changed) {
        Ok(reply) => reply,
        Err(error) => error_document(&format!("{error:#}")),
    }
}

fn try_execute_triplets_request<T: Triplets + ?Sized>(
    triplets: &mut T,
    document: &[LiNo<String>],
    changed: &mut Vec<TripletChange>,
) -> anyhow::Result<LinoDocument> {
    if let Some(operation) = TripletsOperation::from_document(document)? {
        let reply = operation.execute(triplets)?;
        if matches!(
            operation,
            TripletsOperation::Create { .. }
                | TripletsOperation::Update { .. }
                | TripletsOperation::Delete(_)
        ) {
            changed.extend(
                parse_triplet_changes(&reply)?
                    .into_iter()
                    .filter(|change| change.before != change.after),
            );
        }
        return Ok(reply);
    }
    if let Some(operation) = LinksOperation::from_document(document)? {
        return operation.execute(triplets);
    }
    if document.is_empty() {
        return triplets
            .each_triplet(TripletQuery::any())
            .iter()
            .map(|triplet| named_triplet_lino(triplets, triplet))
            .collect();
    }
    let changes = process_triplets_query(triplets, &format_document(document))?;
    let writes = changes
        .iter()
        .filter(|change| change.before != change.after);
    let before = changed.len();
    changed.extend(writes.copied());
    if changed.len() != before {
        triplets.save()?;
    }
    changes
        .iter()
        .map(|change| {
            Ok(group(vec![
                named_side(triplets, change.before.as_ref())?,
                named_side(triplets, change.after.as_ref())?,
            ]))
        })
        .collect()
}

/// `(index: type from to)`, naming every reference that has a name.
fn named_triplet_lino<T: Triplets + ?Sized>(
    triplets: &mut T,
    triplet: &Triplet,
) -> anyhow::Result<LiNo<String>> {
    let mut reference = |id: u32| -> anyhow::Result<String> { reference_name(triplets, id) };
    Ok(LiNo::Link {
        id: Some(reference(triplet.index)?),
        values: vec![
            LiNo::Ref(reference(triplet.type_id)?),
            LiNo::Ref(reference(triplet.from)?),
            LiNo::Ref(reference(triplet.to)?),
        ],
    })
}

fn named_side<T: Triplets + ?Sized>(
    triplets: &mut T,
    triplet: Option<&Triplet>,
) -> anyhow::Result<LiNo<String>> {
    Ok(group(match triplet {
        Some(triplet) => vec![named_triplet_lino(triplets, triplet)?],
        None => Vec::new(),
    }))
}

/// `(index: type from to)` with plain numbers.
pub fn triplet_lino(triplet: &Triplet) -> LiNo<String> {
    named(
        &triplet.index.to_string(),
        vec![
            number(triplet.type_id),
            number(triplet.from),
            number(triplet.to),
        ],
    )
}

fn side_lino(triplet: Option<&Triplet>) -> LiNo<String> {
    group(triplet.map(triplet_lino).into_iter().collect())
}

/// A reply listing `changes`, one `(before) (after)` line each.
pub fn triplet_changes_document(changes: &[TripletChange]) -> LinoDocument {
    changes
        .iter()
        .map(|change| {
            group(vec![
                side_lino(change.before.as_ref()),
                side_lino(change.after.as_ref()),
            ])
        })
        .collect()
}

/// A push: `(notify: N)` followed by one `(before) (after)` line per change.
pub fn triplet_notification_document(subscription: u32, changes: &[TripletChange]) -> LinoDocument {
    let mut document = vec![named("notify", vec![number(subscription)])];
    document.extend(triplet_changes_document(changes));
    document
}

/// The subscription and the changes of a [`triplet_notification_document`].
pub fn parse_triplet_notification(
    document: &[LiNo<String>],
) -> ProtocolResult<TripletsNotification> {
    let subscription =
        notification_number(document).ok_or_else(|| malformed_reply("notify", document))?;
    Ok(TripletsNotification {
        subscription,
        changes: parse_triplet_changes(&document[1..])?,
    })
}

/// The triplets of an `each` reply.
pub fn parse_triplets(document: &[LiNo<String>]) -> ProtocolResult<Vec<Triplet>> {
    document.iter().map(parse_triplet).collect()
}

/// The `(before) (after)` pairs of a `create`, `update` or `delete` reply.
pub fn parse_triplet_changes(document: &[LiNo<String>]) -> ProtocolResult<Vec<TripletChange>> {
    document
        .iter()
        .map(|change| match parts(change) {
            [before, after] => Ok(TripletChange {
                before: parse_side(before)?,
                after: parse_side(after)?,
            }),
            _ => Err(malformed_reply("change", document)),
        })
        .collect()
}

fn parse_side(side: &LiNo<String>) -> ProtocolResult<Option<Triplet>> {
    match side {
        LiNo::Link { id: None, values } if values.is_empty() => Ok(None),
        LiNo::Link { id: None, values } if values.len() == 1 => parse_triplet(&values[0]).map(Some),
        // `((index: type from to))` loses its wrapper when the side is the
        // single named link itself.
        link @ LiNo::Link { id: Some(_), .. } => parse_triplet(link).map(Some),
        _ => Err(malformed_argument("a change side", side)),
    }
}

/// A triplet, as `(index: type from to)` or as [`pack_triplets`] writes it.
pub fn parse_triplet(link: &LiNo<String>) -> ProtocolResult<Triplet> {
    let malformed = || {
        ProtocolError::malformed(format!(
            "expected (index: type from to), found {}",
            format_link(link)
        ))
    };
    let LiNo::Link {
        id: Some(index),
        values,
    } = link
    else {
        return Err(malformed());
    };
    let values: &[LiNo<String>] = match values.as_slice() {
        [LiNo::Link { id: None, values }] => values,
        values => values,
    };
    let [type_id, from, to] = values else {
        return Err(malformed());
    };
    Ok(Triplet::new(
        parse_text_number(index)?,
        parse_number(type_id)?,
        parse_number(from)?,
        parse_number(to)?,
    ))
}

/// Rewrites every triplet of `document` so it fits `format`: in a binary format
/// whose arity holds three references but not five, `(index: type from to)`
/// becomes `(index: (type from to))`, whose second part travels as one link of
/// three references. Under any other format the document stays as it is.
///
/// `(index: type from to)` needs a link of five references, the marker of a
/// named link and the index included, so under `2..3` the mapping would spell
/// it as a chain of doublets: nothing would gain from the arity.
pub fn pack_triplets(document: LinoDocument, format: MessageFormat) -> LinoDocument {
    match format {
        MessageFormat::Binary(options)
            if options.arity.contains(3) && !options.arity.contains(5) =>
        {
            document.into_iter().map(pack_link).collect()
        }
        _ => document,
    }
}

fn pack_link(link: LiNo<String>) -> LiNo<String> {
    match link {
        LiNo::Link { id: None, values } => group(values.into_iter().map(pack_link).collect()),
        LiNo::Link {
            id: Some(id),
            values,
        } if values.len() == 3 => LiNo::Link {
            id: Some(id),
            values: vec![group(values)],
        },
        link => link,
    }
}

/// The inverse of [`pack_triplets`]: the canonical text of a triplet reply
/// does not depend on the format that carried it.
pub fn canonical_triplets(document: LinoDocument) -> LinoDocument {
    document.into_iter().map(unpack_link).collect()
}

fn unpack_link(link: LiNo<String>) -> LiNo<String> {
    match link {
        LiNo::Link { id: None, values } => group(values.into_iter().map(unpack_link).collect()),
        LiNo::Link {
            id: Some(id),
            mut values,
        } if matches!(
            values.as_slice(),
            [LiNo::Link { id: None, values }] if values.len() == 3
        ) =>
        {
            let Some(LiNo::Link { values, .. }) = values.pop() else {
                unreachable!("the guard holds one unnamed link");
            };
            LiNo::Link {
                id: Some(id),
                values,
            }
        }
        link => link,
    }
}
