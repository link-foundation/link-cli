//! Canonical LiNo text for documents, shared by every protocol.
//!
//! `links_notation::format_links` does not quote references that contain
//! spaces or other delimiters, so it cannot be used to put a decoded binary
//! message back on the wire. This formatter guarantees that
//! `parse(format(document)) == document` for every parsed document, which is
//! what makes the text and binary protocols interchangeable.

use super::error::{ProtocolError, ProtocolResult};
use super::mapping::LinoDocument;
use links_notation::{parse_lino_to_links, LiNo};

/// Parses LiNo text into a canonical document. Blank input is the empty document.
pub fn parse_document(text: &str) -> ProtocolResult<LinoDocument> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let links =
        parse_lino_to_links(text).map_err(|error| ProtocolError::InvalidLino(error.to_string()))?;
    Ok(links.into_iter().map(canonical).collect())
}

/// Converts a parsed link into the canonical model: an unnamed group holding
/// exactly one reference is that reference, so `a` and `(a)` both parse as
/// the reference `a`. links-notation (since 0.17) and its C# port keep that
/// wrapper; removing it gives both ports the same document model.
pub fn canonical(link: LiNo<String>) -> LiNo<String> {
    match link {
        LiNo::Link {
            id: None,
            mut values,
        } if values.len() == 1 && matches!(values[0], LiNo::Ref(_)) => {
            values.pop().expect("one value")
        }
        LiNo::Link { id, values } => LiNo::Link {
            id,
            values: values.into_iter().map(canonical).collect(),
        },
        reference => reference,
    }
}

/// Formats a document as canonical LiNo text, one top-level link per line.
///
/// A top-level link without an id and with at least two values is written
/// without its outer parentheses, the way queries are usually typed:
/// `() ((1 1))`.
pub fn format_document(document: &[LiNo<String>]) -> String {
    document
        .iter()
        .map(format_top_level)
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_top_level(link: &LiNo<String>) -> String {
    match link {
        LiNo::Link { id: None, values } if values.len() >= 2 => join_values(values),
        link => format_link(link),
    }
}

/// Formats one link as it appears nested inside another link.
pub fn format_link(link: &LiNo<String>) -> String {
    match link {
        LiNo::Ref(reference) => format_reference(reference),
        LiNo::Link { id: None, values } => match values.as_slice() {
            // `(a)` parses back as the reference `a`, so a one-reference
            // link needs a second pair of parentheses.
            [LiNo::Ref(reference)] => format!("(({}))", format_reference(reference)),
            values => format!("({})", join_values(values)),
        },
        LiNo::Link {
            id: Some(id),
            values,
        } => {
            if values.is_empty() {
                format!("({}:)", format_reference(id))
            } else {
                format!("({}: {})", format_reference(id), join_values(values))
            }
        }
    }
}

fn join_values(values: &[LiNo<String>]) -> String {
    values.iter().map(format_link).collect::<Vec<_>>().join(" ")
}

/// Quotes a reference when it would not survive parsing as a bare word.
///
/// links-notation opens a quoted reference with a run of `N` equal quote
/// characters, closes it with the next run of exactly `N`, and reads `2N`
/// quotes inside as `N` literal ones. The opening run is counted greedily, so
/// the chosen quote must differ from the first character; `N` is one more
/// than the longest run of that quote inside, and odd, because an even
/// delimiter run may be read as an empty reference.
pub fn format_reference(reference: &str) -> String {
    let needs_quotes = reference.is_empty()
        || reference.chars().any(|character| {
            character.is_whitespace() || matches!(character, '(' | ')' | ':' | '\'' | '"' | '`')
        });
    if !needs_quotes {
        return reference.to_string();
    }
    let first = reference.chars().next();
    let (quote, count) = ['\'', '"', '`']
        .into_iter()
        .filter(|&quote| first != Some(quote))
        .map(|quote| (quote, (longest_run(reference, quote) + 1) | 1))
        .min_by_key(|&(_, count)| count)
        .expect("a reference starts with at most one of three quote characters");
    let delimiter = quote.to_string().repeat(count);
    format!("{delimiter}{reference}{delimiter}")
}

fn longest_run(text: &str, quote: char) -> usize {
    let (mut longest, mut current) = (0, 0);
    for character in text.chars() {
        current = if character == quote { current + 1 } else { 0 };
        longest = longest.max(current);
    }
    longest
}
