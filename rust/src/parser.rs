//! LiNo Parser - Parses LiNo notation into LinoLink structures
//!
//! This module provides the Parser for LiNo notation, corresponding to
//! Platform.Protocols.Lino.Parser in C#.

use crate::error::LinkError;
use crate::lino_link::LinoLink;

/// Parser for LiNo notation
/// Corresponds to Platform.Protocols.Lino.Parser in C#
pub struct Parser;

impl Parser {
    /// Creates a new Parser instance
    pub fn new() -> Self {
        Self
    }

    /// Parses a LiNo query string into a list of LinoLinks
    pub fn parse(&self, query: &str) -> Result<Vec<LinoLink>, LinkError> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(vec![]);
        }

        let mut result = Vec::new();
        let mut pos = 0;
        let chars: Vec<char> = query.chars().collect();

        while pos < chars.len() {
            self.skip_whitespace(&chars, &mut pos);
            if pos >= chars.len() {
                break;
            }

            if chars[pos] == '(' {
                let link = self.parse_link(&chars, &mut pos)?;
                result.push(link);
            } else {
                // Handle top-level identifiers
                let id = self.parse_identifier(&chars, &mut pos)?;
                result.push(LinoLink::new(Some(id)));
            }
        }

        Ok(result)
    }

    /// Parses a single link starting at the given position
    fn parse_link(&self, chars: &[char], pos: &mut usize) -> Result<LinoLink, LinkError> {
        if *pos >= chars.len() || chars[*pos] != '(' {
            return Err(LinkError::ParseError(
                "Expected '(' at start of link".to_string(),
            ));
        }
        *pos += 1; // consume '('

        self.skip_whitespace(chars, pos);

        let mut id: Option<String> = None;
        let mut values: Vec<LinoLink> = Vec::new();

        // Parse content until ')'
        while *pos < chars.len() && chars[*pos] != ')' {
            self.skip_whitespace(chars, pos);

            if *pos >= chars.len() || chars[*pos] == ')' {
                break;
            }

            if chars[*pos] == '(' {
                // Nested link
                let nested = self.parse_link(chars, pos)?;
                values.push(nested);
            } else {
                // Identifier or ID
                let identifier = self.parse_identifier(chars, pos)?;

                // Check if this is an ID (ends with ':')
                if identifier.ends_with(':') {
                    // This is the link's ID
                    let clean_id = identifier.trim_end_matches(':').to_string();
                    id = Some(clean_id);
                } else {
                    // This is a value
                    values.push(LinoLink::new(Some(identifier)));
                }
            }

            self.skip_whitespace(chars, pos);
        }

        // Consume ')'
        if *pos < chars.len() && chars[*pos] == ')' {
            *pos += 1;
        }

        // If no explicit ID but we have values, the first non-nested element might be the ID
        // This handles cases like "(id source target)" where id is the index
        if id.is_none() && !values.is_empty() {
            // Check if first value could be an ID (single identifier, not a nested link)
            let first = &values[0];
            if !first.has_values() && first.id.is_some() {
                // Don't auto-promote to ID - keep as first value
            }
        }

        if values.is_empty() && id.is_some() {
            Ok(LinoLink::new(id))
        } else if values.is_empty() {
            Ok(LinoLink::default())
        } else {
            Ok(LinoLink::with_values(id, values))
        }
    }

    /// Parses an identifier (name, number, variable, or wildcard)
    fn parse_identifier(&self, chars: &[char], pos: &mut usize) -> Result<String, LinkError> {
        self.skip_whitespace(chars, pos);

        if *pos >= chars.len() {
            return Err(LinkError::ParseError("Unexpected end of input".to_string()));
        }

        let start = *pos;

        // Handle quoted strings
        if chars[*pos] == '"' || chars[*pos] == '\'' {
            let quote = chars[*pos];
            *pos += 1;
            while *pos < chars.len() && chars[*pos] != quote {
                if chars[*pos] == '\\' && *pos + 1 < chars.len() {
                    *pos += 2; // Skip escaped character
                } else {
                    *pos += 1;
                }
            }
            if *pos < chars.len() {
                *pos += 1; // consume closing quote
            }
            let content: String = chars[start + 1..*pos - 1].iter().collect();
            return Ok(content);
        }

        // Handle regular identifiers
        while *pos < chars.len() {
            let c = chars[*pos];
            if c.is_whitespace() || c == '(' || c == ')' {
                break;
            }
            *pos += 1;
        }

        let identifier: String = chars[start..*pos].iter().collect();
        Ok(identifier)
    }

    /// Skips whitespace characters
    fn skip_whitespace(&self, chars: &[char], pos: &mut usize) {
        while *pos < chars.len() && chars[*pos].is_whitespace() {
            *pos += 1;
        }
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty() {
        let parser = Parser::new();
        let result = parser.parse("").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_simple_link() {
        let parser = Parser::new();
        let result = parser.parse("(1 2)").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].values_count(), 2);
    }

    #[test]
    fn test_parse_nested_link() {
        let parser = Parser::new();
        let result = parser.parse("((1 2) (3 4))").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].values_count(), 2);
    }

    #[test]
    fn test_parse_link_with_id() {
        let parser = Parser::new();
        let result = parser.parse("(5: 1 2)").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, Some("5".to_string()));
        assert_eq!(result[0].values_count(), 2);
    }

    #[test]
    fn test_parse_variable() {
        let parser = Parser::new();
        let result = parser.parse("($var 1 2)").unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].values.as_ref().unwrap()[0].is_variable());
    }

    #[test]
    fn test_parse_wildcard() {
        let parser = Parser::new();
        let result = parser.parse("(* 1)").unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].values.as_ref().unwrap()[0].is_wildcard());
    }

    #[test]
    fn test_parse_query_format() {
        let parser = Parser::new();
        // Query format: (( restriction ) ( substitution ))
        let result = parser.parse("(((1 2)) ((3 4)))").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].values_count(), 2);
    }
}
