//! Query reference kinds layered over the notation parser's string-only model.
//!
//! Every name receives an internal quote envelope (including names containing quotes),
//! removed only at name lookups/writes. Bare addresses, variables and wildcards
//! keep their syntax. This prevents quoted numbers from reaching address parsing.

use crate::{error::LinkError, lino_link::LinoLink, parser::Parser};
use links_notation::{
    comments::strip_comments, encode_reference_literal, parse_lino_to_links,
    parser::quoted_reference_end, LiNo,
};

pub(crate) fn name(identifier: &str) -> &str {
    identifier
        .strip_prefix('"')
        .and_then(|text| text.strip_suffix('"'))
        .unwrap_or(identifier)
}

// A normalised trigger is parsed again when replayed, so names must stay names.
pub(crate) fn format_identifier(identifier: &str) -> String {
    let value = name(identifier);
    if value == identifier {
        return identifier.to_string();
    }
    let safe_bare_name = !value.is_empty()
        && value.parse::<u32>().is_err()
        && value != "*"
        && !value.starts_with(['$', '#', '~'])
        && !value.chars().any(|character| {
            character.is_whitespace() || character.is_control() || "():\"'`".contains(character)
        });
    if safe_bare_name {
        value.to_string()
    } else if value.trim().is_empty()
        || value == "*"
        || value.starts_with(['$', '#', '~'])
        || value.contains('`')
        || value.chars().any(char::is_control)
        || (value.contains('\'') && value.contains('"'))
    {
        encode_reference_literal(value)
    } else {
        // Trigger identity is stored as text. Keep the established quoting for
        // ordinary names so existing stored triggers can still be removed.
        crate::named_type_links::escape_lino_reference(value)
    }
}

pub(crate) fn parse(query: &str) -> Result<Vec<LinoLink>, LinkError> {
    let parser = Parser::new();
    // Preserve diagnostics from the original source, before introducing envelopes.
    let original = parser.parse(query)?;
    if original.is_empty() {
        return Ok(original);
    }
    let text = strip_comments(query);
    let mut rewritten = String::new();
    let mut position = 0;
    while position < text.len() {
        let current = text.as_bytes()[position];
        if b" \t\r\n(:)".contains(&current) {
            rewritten.push(char::from(current));
            position += 1;
            continue;
        }
        let start = position;
        let quoted_end = if b"\"'`".contains(&current) {
            quoted_reference_end(&text, start)
        } else {
            None
        };
        let quoted = quoted_end.is_some();
        if let Some(end) = quoted_end {
            position = end;
        } else {
            // The notation grammar falls back to a bare word for an unmatched
            // odd quote prefix; that quote is part of the name.
            while position < text.len() && !b" \t\r\n(:)".contains(&text.as_bytes()[position]) {
                position += 1;
            }
        }
        let token = &text[start..position];
        let literal = token.strip_prefix('~').is_some_and(|rest| {
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            digits > 0 && rest.as_bytes().get(digits) == Some(&b'{')
        });
        let is_syntax = !quoted
            && !literal
            && (token.parse::<u32>().is_ok() || token == "*" || token.starts_with('$'));
        if is_syntax {
            rewritten.push_str(token);
        } else {
            let value = if quoted || literal {
                let links = parse_lino_to_links(token)
                    .map_err(|error| LinkError::ParseError(error.to_string()))?;
                single_reference(&links[0]).to_string()
            } else {
                token.to_string()
            };
            rewritten.push_str(&encode_reference_literal(&format!("\"{value}\"")));
        }
    }
    parser.parse(&rewritten)
}

fn single_reference(link: &LiNo<String>) -> &str {
    match link {
        LiNo::Ref(value) => value,
        LiNo::Link {
            id: Some(value), ..
        } => value,
        LiNo::Link { values, .. } if values.len() == 1 => single_reference(&values[0]),
        _ => "",
    }
}
