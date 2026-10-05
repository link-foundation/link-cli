use anyhow::Result;
use link_cli::{Link, LinkStorage, QueryProcessor};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[derive(Debug, Clone, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ClinkOptions {
    pub trace: bool,
    pub auto_create_missing_references: bool,
    pub structure: Option<u32>,
    pub before: bool,
    pub changes: bool,
    pub after: bool,
}

impl Default for ClinkOptions {
    fn default() -> Self {
        Self {
            trace: false,
            auto_create_missing_references: true,
            structure: None,
            before: false,
            changes: true,
            after: true,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ClinkResult {
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
    pub links: Vec<WebLink>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WebLink {
    pub id: u32,
    pub source: u32,
    pub target: u32,
    pub name: Option<String>,
}

#[wasm_bindgen]
pub struct Clink {
    /// The same store as the CLI's, kept in memory: the browser has no files.
    storage: LinkStorage,
}

#[wasm_bindgen]
impl Clink {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Clink {
        set_panic_hook();
        Clink {
            storage: LinkStorage::in_memory(false),
        }
    }

    #[wasm_bindgen]
    pub fn execute(&mut self, query: &str, options_json: &str) -> String {
        to_json(&match self.execute_inner(query, options_json) {
            Ok(result) => result,
            Err(error) => ClinkResult {
                success: false,
                output: String::new(),
                error: Some(error.to_string()),
                links: snapshot(&self.storage),
            },
        })
    }

    #[wasm_bindgen]
    pub fn snapshot(&mut self) -> String {
        to_json(&ClinkResult {
            success: true,
            output: self.storage.lino_lines().join("\n"),
            error: None,
            links: snapshot(&self.storage),
        })
    }

    #[wasm_bindgen]
    pub fn reset(&mut self) -> String {
        self.storage = LinkStorage::in_memory(false);
        self.snapshot()
    }

    #[wasm_bindgen]
    pub fn version() -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    #[wasm_bindgen(js_name = rustCoreVersion)]
    pub fn rust_core_version() -> String {
        link_cli::cli::Cli::version_text()
    }

    #[wasm_bindgen]
    pub fn test() -> bool {
        true
    }
}

impl Default for Clink {
    fn default() -> Self {
        Self::new()
    }
}

impl Clink {
    fn execute_inner(&mut self, query: &str, options_json: &str) -> Result<ClinkResult> {
        let options = parse_options(options_json)?;
        let mut output = Vec::new();

        if let Some(structure_id) = options.structure {
            output.push(self.storage.format_structure(structure_id)?);
            return Ok(self.result(output, true, None));
        }

        if options.before {
            output.extend(self.storage.lino_lines());
        }

        if !query.trim().is_empty() {
            let processor = QueryProcessor::new(options.trace)
                .with_auto_create_missing_references(options.auto_create_missing_references);
            let changes = processor.process_query(&mut self.storage, query)?;
            if options.changes {
                for (before, after) in &changes {
                    output.push(format_change(&self.storage, before, after));
                }
            }
        }

        if options.after {
            output.extend(self.storage.lino_lines());
        }

        Ok(self.result(output, true, None))
    }

    fn result(&self, output: Vec<String>, success: bool, error: Option<String>) -> ClinkResult {
        ClinkResult {
            success,
            output: output.join("\n"),
            error,
            links: snapshot(&self.storage),
        }
    }
}

fn parse_options(options_json: &str) -> Result<ClinkOptions> {
    let trimmed = options_json.trim();
    if trimmed.is_empty() {
        return Ok(ClinkOptions::default());
    }

    serde_json::from_str(trimmed).map_err(|error| anyhow::anyhow!("Invalid options JSON: {error}"))
}

fn to_json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|error| {
        format!(
            r#"{{"success":false,"output":"","error":"Failed to serialize result: {error}","links":[]}}"#
        )
    })
}

fn set_panic_hook() {
    #[cfg(all(feature = "console_error_panic_hook", target_arch = "wasm32"))]
    console_error_panic_hook::set_once();
}

/// The links of `storage` with their names, ordered by address.
fn snapshot(storage: &LinkStorage) -> Vec<WebLink> {
    storage
        .all()
        .into_iter()
        .map(|link| WebLink {
            id: link.index,
            source: link.source,
            target: link.target,
            name: storage.get_name(link.index).cloned(),
        })
        .collect()
}

/// A change as `(before) (after)`, each side in LiNo, empty when absent.
fn format_change(storage: &LinkStorage, before: &Option<Link>, after: &Option<Link>) -> String {
    let format = |link: &Option<Link>| {
        link.map(|link| storage.format_lino(&link))
            .unwrap_or_default()
    };
    format!("({}) ({})", format(before), format(after))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn executes_queries_with_the_rust_core() {
        let mut clink = Clink::new();
        let raw = clink.execute(
            "() ((child: father mother))",
            r#"{"changes":true,"after":true}"#,
        );
        let parsed: Value = serde_json::from_str(&raw).unwrap();

        assert_eq!(parsed["success"], true);
        assert!(parsed["output"].as_str().unwrap().contains("child"));
        assert_eq!(parsed["links"].as_array().unwrap().len(), 3);
    }

    fn run(clink: &mut Clink, query: &str) -> String {
        let raw = clink.execute(query, r#"{"changes":true,"after":true}"#);
        let parsed: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed["success"], true, "{raw}");
        parsed["output"].as_str().unwrap().to_string()
    }

    #[test]
    fn deleting_a_link_deletes_its_usages_as_the_cli_does() {
        let mut clink = Clink::new();
        run(&mut clink, "() ((1 1) (2 2) (1 2))");

        assert_eq!(
            run(&mut clink, "((2: 2 2)) ()"),
            "((2: 2 2)) ()\n((3: 1 2)) ()\n(1: 1 1)"
        );
    }

    #[test]
    fn updating_a_link_into_an_existing_pair_merges_them_as_the_cli_does() {
        let mut clink = Clink::new();
        run(&mut clink, "() ((1 1) (2 2) (1 2) (2 1))");

        assert_eq!(
            run(&mut clink, "(((4: 2 1)) ((4: 1 2)))"),
            "((4: 2 1)) ()\n(1: 1 1)\n(2: 2 2)\n(3: 1 2)"
        );
    }

    #[test]
    fn reset_empties_the_store() {
        let mut clink = Clink::new();
        run(&mut clink, "() ((named: named named))");
        let parsed: Value = serde_json::from_str(&clink.snapshot()).unwrap();
        assert_eq!(parsed["links"][0]["name"], "named");

        let parsed: Value = serde_json::from_str(&clink.reset()).unwrap();
        assert_eq!(parsed["links"].as_array().unwrap().len(), 0);
        assert_eq!(parsed["output"], "");
    }

    #[test]
    fn rejects_invalid_options() {
        let mut clink = Clink::new();
        let raw = clink.execute("() ((1 1))", "not json");
        let parsed: Value = serde_json::from_str(&raw).unwrap();

        assert_eq!(parsed["success"], false);
        assert!(parsed["error"]
            .as_str()
            .unwrap()
            .contains("Invalid options JSON"));
    }
}
