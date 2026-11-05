//! Triplets in Links Notation: one `(index: type from to)` per line.
//!
//! The export lists every triplet in the order of its address, a part by its
//! name when it has one and `0` for an empty from or to. The import reads the
//! same lines back: a numeric index is kept, a named index is the triplet of
//! that name (created when there is none), and a line without an index is a
//! new triplet. Lines may refer to each other in any order, since every
//! triplet is reserved before any of them gets its parts.

use anyhow::{bail, Result};

use super::query::{pattern_of, substitute, Reference};
use super::{Triplet, TripletQuery, Triplets};
use crate::query_parser;

/// Every triplet as a line `(index: type from to)`, in the order of the
/// addresses.
pub fn triplets_to_lino<T: Triplets + ?Sized>(triplets: &mut T) -> Result<String> {
    let mut text = String::new();
    for Triplet {
        index,
        type_id,
        from,
        to,
    } in triplets.each_triplet(TripletQuery::any())
    {
        text.push_str(&format!(
            "({}: {} {} {})\n",
            reference(triplets, index)?,
            reference(triplets, type_id)?,
            reference(triplets, from)?,
            reference(triplets, to)?
        ));
    }
    Ok(text)
}

fn reference<T: Triplets + ?Sized>(triplets: &mut T, id: u32) -> Result<String> {
    if id == 0 {
        return Ok("0".to_string());
    }
    Ok(match triplets.get_name(id)? {
        Some(name) => query_parser::format_identifier(&format!("\"{name}\"")),
        None => id.to_string(),
    })
}

/// Writes the triplets of `text` and returns their indices, one per line.
pub fn import_triplets_lino<T: Triplets + ?Sized>(
    triplets: &mut T,
    text: &str,
) -> Result<Vec<u32>> {
    let parsed = query_parser::parse(text)?;
    let mut patterns = Vec::with_capacity(parsed.len());
    for line in &parsed {
        // A document of one line is that line wrapped once more.
        let line = match line.values.as_deref() {
            Some([only]) if line.id.is_none() && only.has_values() => only,
            _ => line,
        };
        let pattern = pattern_of(line)?;
        let references = pattern.index.iter().chain(pattern.parts.iter());
        for reference in references {
            if matches!(reference, Reference::Any | Reference::Variable(_)) {
                bail!("A triplet in Links Notation has no variables or wildcards.");
            }
        }
        patterns.push(pattern);
    }
    let mut changes = Vec::new();
    substitute(triplets, &patterns, Default::default(), &mut changes)
}
