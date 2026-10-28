//! The options one query is processed with.

/// Options for query processing
pub struct QueryOptions {
    pub query: String,
    pub trace: bool,
}

impl QueryOptions {
    pub fn new(query: &str, trace: bool) -> Self {
        Self {
            query: query.to_string(),
            trace,
        }
    }
}

/// Admission limits selected by a checked storage adapter. Existing CLI
/// stores retain their established behavior when no limits are selected.
#[derive(Clone, Copy, Debug)]
pub struct QueryLimits {
    pub max_address: u128,
    pub max_bytes: usize,
    pub max_depth: usize,
    pub max_nodes: usize,
    pub max_results: usize,
    pub max_bindings: usize,
    pub max_binding_bytes: usize,
}

impl QueryLimits {
    /// Finite limits for a request over a complete u64 causal projection.
    pub const fn causal() -> Self {
        Self {
            max_address: u64::MAX as u128,
            max_bytes: 1024 * 1024,
            max_depth: 64,
            max_nodes: 4096,
            max_results: 4096,
            max_bindings: 16_384,
            max_binding_bytes: 4 * 1024 * 1024,
        }
    }
}
