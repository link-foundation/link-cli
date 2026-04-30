//! LiNo Parser - Parses LiNo notation into LinoLink structures.
//!
//! This module adapts the upstream `links-notation` parser into the local
//! `LinoLink` representation used by the query processor.

use crate::error::LinkError;
use crate::lino_link::LinoLink;
use links_notation::{parse_lino_to_links, LiNo};

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
        parse_lino_to_links(query)
            .map(|links| links.into_iter().map(Self::convert_link).collect())
            .map_err(|error| LinkError::ParseError(error.to_string()))
    }

    fn convert_link(link: LiNo<String>) -> LinoLink {
        match link {
            LiNo::Ref(id) => LinoLink::new(Some(id)),
            LiNo::Link { id, values } if values.is_empty() => {
                id.map(|id| LinoLink::new(Some(id))).unwrap_or_default()
            }
            LiNo::Link { id, values } => {
                LinoLink::with_values(id, values.into_iter().map(Self::convert_link).collect())
            }
        }
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}
