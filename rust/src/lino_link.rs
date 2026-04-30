//! LinoLink - A parsed LiNo link structure
//!
//! This module provides the LinoLink data structure that represents
//! a parsed link from LiNo notation.

/// LinoLink represents a parsed link from LiNo notation
/// Corresponds to Platform.Protocols.Lino.Link<string> in C#
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LinoLink {
    /// The ID/name of this link (can be a number, variable, or name)
    pub id: Option<String>,
    /// Child values (for composite links)
    pub values: Option<Vec<LinoLink>>,
}

impl LinoLink {
    /// Creates a new LinoLink with just an ID
    pub fn new(id: Option<String>) -> Self {
        Self { id, values: None }
    }

    /// Creates a new LinoLink with an ID and values
    pub fn with_values(id: Option<String>, values: Vec<LinoLink>) -> Self {
        Self {
            id,
            values: Some(values),
        }
    }

    /// Returns true if this link has no ID
    pub fn is_empty(&self) -> bool {
        self.id.is_none() && self.values.as_ref().is_none_or(|v| v.is_empty())
    }

    /// Returns true if this link has child values
    pub fn has_values(&self) -> bool {
        self.values.as_ref().is_some_and(|v| !v.is_empty())
    }

    /// Gets the number of child values
    pub fn values_count(&self) -> usize {
        self.values.as_ref().map_or(0, |v| v.len())
    }

    /// Returns true if the ID is a variable (starts with $)
    pub fn is_variable(&self) -> bool {
        self.id.as_ref().is_some_and(|id| id.starts_with('$'))
    }

    /// Returns true if the ID is a wildcard (*)
    pub fn is_wildcard(&self) -> bool {
        self.id.as_ref().is_some_and(|id| id == "*")
    }

    /// Returns true if the ID is numeric
    pub fn is_numeric(&self) -> bool {
        self.id.as_ref().is_some_and(|id| id.parse::<u32>().is_ok())
    }
}
