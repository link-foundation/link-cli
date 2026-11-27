//! LiNo database import helpers.

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;

use crate::lino_link::LinoLink;
use crate::named_type_links::NamedTypeLinks;
use crate::parser::Parser;

/// Default nesting limit for a LiNo text import.
pub const DEFAULT_IMPORT_DEPTH: usize = links_notation::parser::DEFAULT_MAX_DEPTH;
/// Highest nesting limit supported by the recursive import path.
pub const MAX_IMPORT_DEPTH: usize = 128;

/// Imports and saves a LiNo file with a nesting limit of 64 levels.
pub fn import_lino_file<T, P>(storage: &mut T, path: P) -> Result<()>
where
    T: NamedTypeLinks,
    P: AsRef<Path>,
{
    import_lino_file_with_max_depth(storage, path, DEFAULT_IMPORT_DEPTH)
}

/// Imports and saves a LiNo file with a nesting limit from 1 to 128.
pub fn import_lino_file_with_max_depth<T, P>(
    storage: &mut T,
    path: P,
    max_depth: usize,
) -> Result<()>
where
    T: NamedTypeLinks,
    P: AsRef<Path>,
{
    let path = path.as_ref();
    let text = fs::read_to_string(path)
        .with_context(|| format!("Failed to read LiNo input: {}", path.display()))?;
    import_lino_text_with_max_depth(storage, &text, max_depth)?;
    storage.save()?;
    Ok(())
}

/// Imports LiNo text with a nesting limit of 64 levels, without saving.
pub fn import_lino_text<T>(storage: &mut T, links_notation: &str) -> Result<()>
where
    T: NamedTypeLinks,
{
    import_lino_text_with_max_depth(storage, links_notation, DEFAULT_IMPORT_DEPTH)
}

/// Imports LiNo text with a nesting limit from 1 to 128, without saving.
pub fn import_lino_text_with_max_depth<T>(
    storage: &mut T,
    links_notation: &str,
    max_depth: usize,
) -> Result<()>
where
    T: NamedTypeLinks,
{
    if !(1..=MAX_IMPORT_DEPTH).contains(&max_depth) {
        bail!("LiNo import nesting limit must be an integer from 1 to {MAX_IMPORT_DEPTH}");
    }
    let normalized_links_notation = normalize_links_notation(links_notation);
    let links = parse_import(&normalized_links_notation, max_depth)?;

    // Parsing accepts more shapes than database import does. Validate the whole
    // tree before resolving references, which may create links and names.
    for link in links.iter().filter(|link| !link.is_empty()) {
        validate_link(link, false)?;
    }

    for link in links.iter().filter(|link| !link.is_empty()) {
        import_link(storage, link)?;
    }

    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_import(text: &str, max_depth: usize) -> Result<Vec<LinoLink>> {
    // The notation parser's recursive frames can exceed a native caller's
    // stack budget, especially in debug builds. Only parsing needs this stack.
    std::thread::scope(|scope| {
        let parser = std::thread::Builder::new()
            .stack_size(8 * 1024 * 1024)
            .spawn_scoped(scope, || {
                Parser::new().parse_with_max_depth(text, max_depth)
            })
            .context("Failed to start the LiNo import parser")?;
        match parser.join() {
            Ok(parsed) => Ok(parsed?),
            Err(panic) => std::panic::resume_unwind(panic),
        }
    })
}

// WebAssembly has no threads, so the parse uses the module's shadow stack.
// The wasm wrapper (rust/wasm/build.rs) links with an 8 MiB stack to match the
// native parse thread; other wasm32 embedders need a similar `-zstack-size`.
#[cfg(target_arch = "wasm32")]
fn parse_import(text: &str, max_depth: usize) -> Result<Vec<LinoLink>> {
    Ok(Parser::new().parse_with_max_depth(text, max_depth)?)
}

fn validate_link(link: &LinoLink, allow_anonymous_index: bool) -> Result<()> {
    let values = link
        .values
        .as_ref()
        .filter(|values| values.len() == 2)
        .ok_or_else(|| anyhow::anyhow!("LiNo import supports links with exactly two values"))?;

    for reference in values {
        match reference.values_count() {
            2 => validate_link(reference, true)?,
            0 => {
                if normalize_identifier(reference.id.as_deref()).is_empty() {
                    bail!("LiNo import references must have a value");
                }
            }
            _ => bail!(
                "LiNo import references must be scalar values or nested links with exactly two values"
            ),
        }
    }

    if !allow_anonymous_index && normalize_identifier(link.id.as_deref()).is_empty() {
        bail!("Top-level LiNo import links must have an index or name");
    }
    Ok(())
}

/// Imports a link `validate_link` accepted, as every entry to this module does first.
fn import_link<T>(storage: &mut T, link: &LinoLink) -> Result<u32>
where
    T: NamedTypeLinks,
{
    let values = link
        .values
        .as_ref()
        .filter(|values| values.len() == 2)
        .ok_or_else(|| anyhow::anyhow!("LiNo import supports links with exactly two values"))?;

    let source = resolve_reference(storage, &values[0])?;
    let target = resolve_reference(storage, &values[1])?;
    let identifier = normalize_identifier(link.id.as_deref());

    if identifier.is_empty() {
        return storage.try_get_or_create(source, target);
    }

    let index = resolve_index(storage, &identifier)?;
    update_link(storage, index, source, target)?;
    Ok(index)
}

fn resolve_reference<T>(storage: &mut T, reference: &LinoLink) -> Result<u32>
where
    T: NamedTypeLinks,
{
    if reference.values_count() == 2 {
        return import_link(storage, reference);
    }

    let identifier = normalize_identifier(reference.id.as_deref());
    if let Some(id) = parse_reference_number(&identifier, true) {
        return Ok(id);
    }

    storage.get_or_create_named(&identifier)
}

fn resolve_index<T>(storage: &mut T, identifier: &str) -> Result<u32>
where
    T: NamedTypeLinks,
{
    if let Some(id) = parse_reference_number(identifier, false) {
        if !storage.exists(id) {
            storage.try_ensure_created(id)?;
        }

        return Ok(id);
    }

    storage.get_or_create_named(identifier)
}

pub(crate) fn update_link<T>(storage: &mut T, index: u32, source: u32, target: u32) -> Result<()>
where
    T: NamedTypeLinks,
{
    if !storage.exists(index) {
        storage.try_ensure_created(index)?;
    }

    if let Some(current) = storage.get_link(index) {
        if current.source == source && current.target == target {
            return Ok(());
        }
    }

    storage.update(index, source, target)?;
    Ok(())
}

fn parse_reference_number(identifier: &str, allow_zero: bool) -> Option<u32> {
    let id = identifier.parse::<u32>().ok()?;
    if id == 0 && !allow_zero {
        return None;
    }

    Some(id)
}

fn normalize_identifier(identifier: Option<&str>) -> String {
    let normalized = identifier
        .unwrap_or_default()
        .trim()
        .trim_end_matches(':')
        .to_string();

    if normalized.len() >= 2 && normalized.starts_with('\'') && normalized.ends_with('\'') {
        return normalized[1..normalized.len() - 1].replace("\\'", "'");
    }

    if normalized.len() >= 2 && normalized.starts_with('"') && normalized.ends_with('"') {
        return normalized[1..normalized.len() - 1].replace("\\\"", "\"");
    }

    normalized
}

fn normalize_links_notation(links_notation: &str) -> String {
    links_notation
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}
