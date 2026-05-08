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
