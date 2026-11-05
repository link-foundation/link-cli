//! Substitution queries over triplets: `((restriction) (substitution))`,
//! where every pattern has three parts, `(index: type from to)` or
//! `(type from to)`.
//!
//! - A restriction pattern matches triplets; the patterns are a conjunction
//!   over their variables (`$name`), and `*` matches anything.
//! - A substitution pattern whose index is the index of a matched triplet
//!   updates it, keeping its address; `*` in a part keeps that part.
//! - A matched triplet no substitution pattern names is deleted, with the
//!   triplets that use it.
//! - Any other substitution pattern creates a triplet: at its address, under
//!   its name, bound to its variable, or anywhere when it has no index.
//!
//! So `((i: T f t)) ((i: T f t2))` moves the to-part of `i`,
//! `(() ((T f t)))` creates, `(((i: T f t)) ())` deletes and a query whose
//! substitution repeats its restriction only reads.

use std::collections::{HashMap, HashSet};

use anyhow::{bail, Result};

use super::{Triplet, TripletQuery, Triplets};
use crate::lino_link::LinoLink;
use crate::query_parser;

/// One change a query made: a creation has no `before`, a deletion no
/// `after`, and a read reports the same triplet on both sides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TripletChange {
    pub before: Option<Triplet>,
    pub after: Option<Triplet>,
}

/// A reference in a pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Reference {
    Any,
    Variable(String),
    Address(u32),
    Name(String),
}

impl Reference {
    fn parse(text: &str) -> Self {
        let name = query_parser::name(text);
        if name != text {
            Self::Name(name.to_string())
        } else if text == "*" {
            Self::Any
        } else if text.starts_with('$') {
            Self::Variable(text.to_string())
        } else if let Ok(address) = text.parse::<u32>() {
            Self::Address(address)
        } else {
            Self::Name(text.to_string())
        }
    }
}

/// `(index: type from to)`, the index optional.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Pattern {
    pub(super) index: Option<Reference>,
    pub(super) parts: [Reference; 3],
}

type Bindings = HashMap<String, u32>;

/// The patterns of one side of a query: `()` has none, `(i: T f t)` is one
/// and `((…) (…))` is a list of them.
pub(super) fn patterns_of(link: &LinoLink) -> Result<Vec<Pattern>> {
    let values = link.values.as_deref().unwrap_or(&[]);
    if link.id.is_some() || values.iter().any(|value| !value.has_values()) {
        return Ok(vec![pattern_of(link)?]);
    }
    values.iter().map(pattern_of).collect()
}

/// The pattern `(index: type from to)` or `(type from to)`.
pub(super) fn pattern_of(link: &LinoLink) -> Result<Pattern> {
    let values = link.values.as_deref().unwrap_or(&[]);
    if values.len() != 3 {
        bail!(
            "A triplet pattern has three parts, type, from and to: {} has {}.",
            describe(link),
            values.len()
        );
    }
    let mut parts = Vec::with_capacity(3);
    for value in values {
        match (&value.id, value.has_values()) {
            (Some(id), false) => parts.push(Reference::parse(id)),
            _ => bail!(
                "A part of a triplet pattern is a reference, not {}.",
                describe(value)
            ),
        }
    }
    let [type_part, from, to]: [Reference; 3] =
        parts.try_into().expect("a pattern has exactly three parts");
    Ok(Pattern {
        index: link.id.as_deref().map(Reference::parse),
        parts: [type_part, from, to],
    })
}

fn describe(link: &LinoLink) -> String {
    let values = link
        .values
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .map(describe)
        .collect::<Vec<_>>()
        .join(" ");
    match (&link.id, link.has_values()) {
        (Some(id), true) => format!("({id}: {values})"),
        (Some(id), false) => id.clone(),
        (None, _) => format!("({values})"),
    }
}

fn name_of<T: Triplets + ?Sized>(triplets: &mut T, name: &str) -> Option<u32> {
    triplets
        .get_by_name(name)
        .ok()
        .flatten()
        .filter(|&id| id != 0)
}

/// Runs a substitution query and returns the changes it made, in order:
/// creations and updates as the substitution lists them, then deletions.
pub fn process_triplets_query<T: Triplets + ?Sized>(
    triplets: &mut T,
    query: &str,
) -> Result<Vec<TripletChange>> {
    let parsed = query_parser::parse(query)?;
    let (restriction, substitution) = match parsed.as_slice() {
        [] => return Ok(Vec::new()),
        [only] => match only.values.as_deref() {
            Some([restriction, substitution]) if only.id.is_none() => (restriction, substitution),
            _ => bail!("A triplets query is ((restriction) (substitution))."),
        },
        [restriction, substitution] => (restriction, substitution),
        _ => bail!("A triplets query is ((restriction) (substitution))."),
    };
    let restrictions = patterns_of(restriction)?;
    let substitutions = patterns_of(substitution)?;
    let mut solutions = Vec::new();
    if restrictions.is_empty() {
        solutions.push((Bindings::new(), Vec::new()));
    } else {
        let mut matched = Vec::new();
        solve(
            triplets,
            &restrictions,
            Bindings::new(),
            &mut matched,
            &mut solutions,
        );
    }
    let mut changes = Vec::new();
    let mut deletes = Vec::new();
    for (bindings, matched) in solutions {
        let kept = substitute(triplets, &substitutions, bindings, &mut changes)?;
        let kept: HashSet<u32> = kept.into_iter().collect();
        for index in matched {
            if !kept.contains(&index) && !deletes.contains(&index) {
                deletes.push(index);
            }
        }
    }
    for index in deletes {
        if triplets.is_triplet(index) {
            for deleted in triplets.delete_triplet(index)? {
                changes.push(TripletChange {
                    before: Some(deleted),
                    after: None,
                });
            }
        }
    }
    Ok(changes)
}

enum Restriction {
    Any,
    Value(u32),
    Impossible,
}

fn restriction_of<T: Triplets + ?Sized>(
    triplets: &mut T,
    reference: &Reference,
    bindings: &Bindings,
) -> Restriction {
    match reference {
        Reference::Any => Restriction::Any,
        Reference::Variable(name) => match bindings.get(name) {
            Some(&value) => Restriction::Value(value),
            None => Restriction::Any,
        },
        Reference::Address(address) => Restriction::Value(*address),
        Reference::Name(name) => match name_of(triplets, name) {
            Some(id) => Restriction::Value(id),
            None => Restriction::Impossible,
        },
    }
}

fn bind(bindings: &mut Bindings, reference: &Reference, value: u32) -> bool {
    match reference {
        Reference::Variable(name) => match bindings.get(name) {
            Some(&bound) => bound == value,
            None => {
                bindings.insert(name.clone(), value);
                true
            }
        },
        _ => true,
    }
}

/// The values of `$variable` under which every pattern of `restriction` matches
/// a triplet, each once, in the order found: a read-only substitution query,
/// as the GraphQL layer asks them (issue #153).
pub fn match_triplets_query<T: Triplets + ?Sized>(
    triplets: &mut T,
    restriction: &str,
    variable: &str,
) -> Result<Vec<u32>> {
    let parsed = query_parser::parse(restriction)?;
    let [link] = parsed.as_slice() else {
        bail!("A restriction is one link of patterns: {restriction}");
    };
    let patterns = patterns_of(link)?;
    let mut solutions = Vec::new();
    solve(
        triplets,
        &patterns,
        Bindings::new(),
        &mut Vec::new(),
        &mut solutions,
    );
    let mut seen = HashSet::new();
    Ok(solutions
        .iter()
        .filter_map(|(bindings, _)| bindings.get(variable).copied())
        .filter(|value| seen.insert(*value))
        .collect())
}

/// Every assignment of the variables under which each restriction pattern
/// matches a triplet, with the triplets they matched.
fn solve<T: Triplets + ?Sized>(
    triplets: &mut T,
    patterns: &[Pattern],
    bindings: Bindings,
    matched: &mut Vec<u32>,
    solutions: &mut Vec<(Bindings, Vec<u32>)>,
) {
    let Some((pattern, rest)) = patterns.split_first() else {
        solutions.push((bindings, matched.clone()));
        return;
    };
    let any = Reference::Any;
    let references = [
        pattern.index.as_ref().unwrap_or(&any),
        &pattern.parts[0],
        &pattern.parts[1],
        &pattern.parts[2],
    ];
    let mut values = [None; 4];
    for (slot, reference) in values.iter_mut().zip(references) {
        match restriction_of(triplets, reference, &bindings) {
            Restriction::Any => {}
            Restriction::Value(value) => *slot = Some(value),
            Restriction::Impossible => return,
        }
    }
    let query = TripletQuery::new(values[0], values[1], values[2], values[3]);
    for triplet in triplets.each_triplet(query) {
        let mut next = bindings.clone();
        let found = [triplet.index, triplet.type_id, triplet.from, triplet.to];
        if references
            .iter()
            .zip(found)
            .all(|(reference, value)| bind(&mut next, reference, value))
        {
            matched.push(triplet.index);
            solve(triplets, rest, next, matched, solutions);
            matched.pop();
        }
    }
}

/// What a substitution pattern writes to.
enum Target {
    Existing(u32),
    Address(u32),
    New(Option<Reference>),
}

/// Applies the substitution patterns under `bindings`: updates the triplets
/// they name, creates the others, and returns the index each pattern wrote,
/// in the order of the patterns.
///
/// Every triplet to create is reserved first as a point `(i: i i)`, which is
/// a triplet, so the patterns may refer to each other in any order; then each
/// gets its parts. Every reference is checked before anything is written.
pub(super) fn substitute<T: Triplets + ?Sized>(
    triplets: &mut T,
    patterns: &[Pattern],
    mut bindings: Bindings,
    changes: &mut Vec<TripletChange>,
) -> Result<Vec<u32>> {
    let mut targets = Vec::with_capacity(patterns.len());
    let mut defined_names = HashSet::new();
    let mut defined_variables = HashSet::new();
    let mut defined_addresses = HashSet::new();
    for pattern in patterns {
        let target = match &pattern.index {
            None => Target::New(None),
            Some(Reference::Any) => bail!("A substitution cannot create or update the index *."),
            Some(Reference::Variable(name)) => match bindings.get(name) {
                Some(&bound) => Target::Existing(bound),
                None => {
                    defined_variables.insert(name.clone());
                    Target::New(pattern.index.clone())
                }
            },
            Some(Reference::Name(name)) => match name_of(triplets, name) {
                Some(id) => Target::Existing(id),
                None => {
                    defined_names.insert(name.clone());
                    Target::New(pattern.index.clone())
                }
            },
            Some(Reference::Address(address)) => {
                if *address == 0 {
                    bail!("A triplet cannot be at address 0.");
                }
                if triplets.is_triplet(*address) {
                    Target::Existing(*address)
                } else if triplets.exists(*address) {
                    bail!("Link {address} is a doublet, not a triplet.");
                } else {
                    defined_addresses.insert(*address);
                    Target::Address(*address)
                }
            }
        };
        if let Target::Existing(index) = target {
            if !triplets.is_triplet(index) {
                bail!("Link {index} is not a triplet.");
            }
        }
        targets.push(target);
    }
    for (pattern, target) in patterns.iter().zip(&targets) {
        for (position, part) in pattern.parts.iter().enumerate() {
            let known = match part {
                Reference::Any => matches!(target, Target::Existing(_)),
                Reference::Variable(name) => {
                    bindings.contains_key(name) || defined_variables.contains(name)
                }
                Reference::Name(name) => {
                    name_of(triplets, name).is_some() || defined_names.contains(name)
                }
                Reference::Address(0) => position > 0,
                Reference::Address(address) => {
                    triplets.reference_is_triplet(*address) || defined_addresses.contains(address)
                }
            };
            if !known {
                bail!(
                    "The {} part {} of a substitution is not a triplet.",
                    ["type", "from", "to"][position],
                    describe_reference(part)
                );
            }
        }
    }

    let mut indices = Vec::with_capacity(targets.len());
    let mut created = HashSet::new();
    for target in targets {
        let index = match target {
            Target::Existing(index) => index,
            Target::Address(address) => {
                if !triplets.is_triplet(address) {
                    triplets.create_triplet_point_at(address)?;
                    created.insert(address);
                }
                address
            }
            Target::New(reference) => {
                // A name or a variable defined twice is one triplet.
                let known = match &reference {
                    Some(Reference::Name(name)) => name_of(triplets, name),
                    Some(Reference::Variable(name)) => bindings.get(name).copied(),
                    _ => None,
                };
                match known {
                    Some(index) => index,
                    None => {
                        let index = triplets.create_triplet_point()?;
                        match &reference {
                            Some(Reference::Name(name)) => {
                                triplets.set_name(index, name)?;
                            }
                            Some(Reference::Variable(name)) => {
                                bindings.insert(name.clone(), index);
                            }
                            _ => {}
                        }
                        created.insert(index);
                        index
                    }
                }
            }
        };
        indices.push(index);
    }

    let mut kept = HashSet::new();
    for (pattern, &index) in patterns.iter().zip(&indices) {
        let before = triplets.get_triplet(index);
        let current = before.unwrap_or(Triplet::new(index, index, 0, 0));
        let currents = [current.type_id, current.from, current.to];
        let mut parts = [0; 3];
        for (position, part) in pattern.parts.iter().enumerate() {
            parts[position] = match part {
                Reference::Any => currents[position],
                Reference::Variable(name) => bindings[name],
                Reference::Address(address) => *address,
                Reference::Name(name) => match name_of(triplets, name) {
                    Some(id) => id,
                    None => bail!("The name {name} is not a triplet."),
                },
            };
        }
        triplets.update_triplet(index, parts[0], parts[1], parts[2])?;
        let after = triplets.get_triplet(index);
        let before = if created.contains(&index) {
            None
        } else {
            before
        };
        // A triplet two patterns write is reported once, as it ends up.
        if let Some(change) = changes.iter_mut().rev().find(|change| {
            change.after.is_some_and(|after| after.index == index) && kept.contains(&index)
        }) {
            change.after = after;
        } else {
            changes.push(TripletChange { before, after });
        }
        kept.insert(index);
    }
    Ok(indices)
}

fn describe_reference(reference: &Reference) -> String {
    match reference {
        Reference::Any => "*".to_string(),
        Reference::Variable(name) | Reference::Name(name) => name.clone(),
        Reference::Address(address) => address.to_string(),
    }
}
