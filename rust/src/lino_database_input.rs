//! LiNo database import helpers.

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;

use crate::lino_link::LinoLink;
use crate::named_type_links::NamedTypeLinks;
use crate::parser::Parser;

pub fn import_lino_file<T, P>(storage: &mut T, path: P) -> Result<()>
where
    T: NamedTypeLinks,
    P: AsRef<Path>,
{
    let path = path.as_ref();
    let text = fs::read_to_string(path)
        .with_context(|| format!("Failed to read LiNo input: {}", path.display()))?;
    import_lino_text(storage, &text)?;
    storage.save()?;
    Ok(())
}

pub fn import_lino_text<T>(storage: &mut T, links_notation: &str) -> Result<()>
where
    T: NamedTypeLinks,
{
    let parser = Parser::new();
    let normalized_links_notation = normalize_links_notation(links_notation);
    let links = parser.parse(&normalized_links_notation)?;

    for link in links.iter().filter(|link| !link.is_empty()) {
        import_link(storage, link, false)?;
    }

    Ok(())
}

fn import_link<T>(storage: &mut T, link: &LinoLink, allow_anonymous_index: bool) -> Result<u32>
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
        if !allow_anonymous_index {
            bail!("Top-level LiNo import links must have an index or name");
        }

        return Ok(storage.get_or_create(source, target));
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
        return import_link(storage, reference, true);
    }

    if reference.values_count() > 0 {
        bail!(
            "LiNo import references must be scalar values or nested links with exactly two values"
        );
    }

    let identifier = normalize_identifier(reference.id.as_deref());
    if identifier.is_empty() {
        bail!("LiNo import references must have a value");
    }

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
            storage.ensure_created(id);
        }

        return Ok(id);
    }

    storage.get_or_create_named(identifier)
}

fn update_link<T>(storage: &mut T, index: u32, source: u32, target: u32) -> Result<()>
where
    T: NamedTypeLinks,
{
    if !storage.exists(index) {
        storage.ensure_created(index);
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
