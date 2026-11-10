//! The legacy text row format and its versioned, line-safe name extension.
use anyhow::{bail, Result};
use links_notation::{decode_reference_literal, encode_reference_literal};

use crate::Link;

pub(crate) const NAME_HEADER: &str = "# link-cli:name:";

/// Controls cannot be written inside a line-oriented row. Legacy quoted text
/// is otherwise literal, including backslashes and literal-looking names.
pub(crate) fn needs_name_metadata(name: &str) -> bool {
    name.chars().any(|c| c < ' ' || c == '\u{7f}')
}

pub(crate) fn name_metadata(id: u32, name: &str) -> String {
    format!("{NAME_HEADER}1 {id} {}", encode_reference_literal(name))
}

pub(crate) fn parse_name_metadata(metadata: &str) -> Result<(u32, String)> {
    let fields = metadata.split_whitespace().collect::<Vec<_>>();
    if fields.len() != 3 || fields[0] != "1" {
        bail!("invalid or unsupported name metadata");
    }
    let id = fields[1];
    if !id.bytes().all(|b| b.is_ascii_digit()) {
        bail!("invalid name metadata address");
    }
    let id = id.parse::<u32>()?;
    let name = decode_reference_literal(fields[2]).map_err(anyhow::Error::msg)?;
    Ok((id, name))
}

pub(crate) fn parse_link_line(line: &str) -> Option<(Link, Option<String>)> {
    // Retain the legacy optional row delimiters. The complete quoted tail
    // below has exactly one enclosing quote pair, preserving quotes in names.
    let mut rest = line.trim_matches(|c| c == '(' || c == ')');
    let index = word(&mut rest)?.parse().ok()?;
    let source = word(&mut rest)?.parse().ok()?;
    let target = word(&mut rest)?.parse().ok()?;
    let tail = rest.trim();
    let name = if tail.is_empty() {
        None
    } else if let Some(quoted) = tail.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        Some(quoted.to_owned())
    } else {
        // Retain the old reader's single-token unquoted spelling. It has no
        // escape syntax: an unquoted ~1{31} is still that literal name.
        Some(word(&mut rest)?.trim_matches('"').to_owned())
    };
    Some((Link::new(index, source, target), name))
}

fn word<'a>(rest: &mut &'a str) -> Option<&'a str> {
    *rest = rest.trim_start();
    if rest.is_empty() {
        return None;
    }
    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let word = &rest[..end];
    *rest = &rest[end..];
    Some(word)
}
