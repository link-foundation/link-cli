//! Shared source-spine formatter. Resolve references in the original unwind order.
use std::collections::HashSet;

use anyhow::Result;

use crate::{error::LinkError, link::GenericLink};

pub(crate) fn format_structure<C: ?Sized, T: doublets::data::LinkReference>(
    context: &mut C,
    mut id: T,
    visited: &mut HashSet<T>,
    mut get: impl FnMut(&mut C, T) -> Option<GenericLink<T>>,
    mut exists: impl FnMut(&mut C, T) -> bool,
    mut reference: impl FnMut(&mut C, T) -> Result<String>,
) -> Result<String> {
    let mut frames = Vec::new();
    let terminal = loop {
        let link = get(context, id).ok_or(LinkError::not_found(id))?;
        if !visited.insert(id) {
            break reference(context, id)?;
        }
        frames.push((id, link));
        if exists(context, link.source) && !visited.contains(&link.source) {
            id = link.source;
        } else {
            break reference(context, link.source)?;
        }
    };

    // Name resolution can fail or have decorator side effects. Preserve its
    // deepest-first target/index order before emitting root-first prefixes.
    let mut parts = Vec::with_capacity(frames.len());
    for (id, link) in frames.into_iter().rev() {
        let target = reference(context, link.target)?;
        let index = reference(context, link.index)?;
        visited.remove(&id);
        parts.push((index, target));
    }
    let mut output = String::new();
    for (index, _) in parts.iter().rev() {
        output.push('(');
        output.push_str(index);
        output.push_str(": ");
    }
    output.push_str(&terminal);
    for (_, target) in parts {
        output.push(' ');
        output.push_str(&target);
        output.push(')');
    }
    Ok(output)
}
