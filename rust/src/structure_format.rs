//! Shared source-spine formatter. Resolve references in the original unwind order.
use std::collections::HashSet;

use anyhow::Result;

use crate::{error::LinkError, link::Link};

pub(crate) fn format_structure<T: ?Sized>(
    context: &mut T,
    mut id: u32,
    visited: &mut HashSet<u32>,
    mut get: impl FnMut(&mut T, u32) -> Option<Link>,
    mut exists: impl FnMut(&mut T, u32) -> bool,
    mut reference: impl FnMut(&mut T, u32) -> Result<String>,
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
