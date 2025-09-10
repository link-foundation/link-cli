mod utils;
mod query_processor;
mod links_operations;
mod lino_parser;

use wasm_bindgen::prelude::*;
use serde::{Deserialize, Serialize};

// When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
// allocator.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[wasm_bindgen]
extern "C" {
    fn alert(s: &str);
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

macro_rules! console_log {
    ($($t:tt)*) => (log(&format_args!($($t)*).to_string()))
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ClinkOptions {
    pub db: Option<String>,
    pub trace: bool,
    pub structure: Option<u64>,
    pub before: bool,
    pub changes: bool,
    pub after: bool,
}

impl Default for ClinkOptions {
    fn default() -> Self {
        Self {
            db: Some("db.links".to_string()),
            trace: false,
            structure: None,
            before: false,
            changes: false,
            after: false,
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ClinkResult {
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
}

#[wasm_bindgen]
pub struct Clink {
    processor: query_processor::QueryProcessor,
}

#[wasm_bindgen]
impl Clink {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Clink {
        utils::set_panic_hook();
        Clink {
            processor: query_processor::QueryProcessor::new(),
        }
    }

    /// Execute a LiNo query and return the result
    #[wasm_bindgen]
    pub fn execute(&mut self, query: &str, options_json: &str) -> String {
        let options: ClinkOptions = match serde_json::from_str(options_json) {
            Ok(opts) => opts,
            Err(e) => {
                let result = ClinkResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("Invalid options JSON: {}", e)),
                };
                return serde_json::to_string(&result).unwrap_or_default();
            }
        };

        match self.processor.execute(query, &options) {
            Ok(output) => {
                let result = ClinkResult {
                    success: true,
                    output,
                    error: None,
                };
                serde_json::to_string(&result).unwrap_or_default()
            }
            Err(error) => {
                let result = ClinkResult {
                    success: false,
                    output: String::new(),
                    error: Some(error),
                };
                serde_json::to_string(&result).unwrap_or_default()
            }
        }
    }

    /// Get version information
    #[wasm_bindgen]
    pub fn version() -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    /// Check if WebAssembly is working correctly
    #[wasm_bindgen]
    pub fn test() -> bool {
        console_log!("clink-wasm test: WebAssembly is working!");
        true
    }
}