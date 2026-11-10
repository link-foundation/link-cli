//! Shared decoded-reference byte admission for checked named query replies.
use crate::{query_address::QueryAddress, query_options::QueryLimits, NamedTypeLinks};
use anyhow::Result;

pub(crate) struct QueryReplyBudget {
    remaining: Option<usize>,
}

impl QueryReplyBudget {
    pub fn new(limits: Option<QueryLimits>) -> Self {
        Self {
            remaining: limits.map(|limits| limits.max_bytes),
        }
    }

    pub fn reference<T: QueryAddress, S: NamedTypeLinks<T> + ?Sized>(
        &mut self,
        storage: &mut S,
        id: T,
    ) -> Result<String> {
        let text = storage.get_name(id)?.unwrap_or_else(|| id.to_string());
        if let Some(remaining) = &mut self.remaining {
            *remaining = remaining
                .checked_sub(text.len())
                .ok_or_else(|| anyhow::anyhow!("query reply byte limit exceeded"))?;
        }
        Ok(text)
    }
}
