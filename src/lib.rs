use std::collections::HashMap;

use anyhow::Result;
use link_cli::{Link, LinkError, NamedTypeLinks, QueryProcessor};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

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
    storage: BrowserStorage,
}

#[wasm_bindgen]
impl Clink {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Clink {
        set_panic_hook();
        Clink {
            storage: BrowserStorage::new(),
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
                links: self.storage.snapshot(),
            },
        })
    }

    #[wasm_bindgen]
    pub fn snapshot(&mut self) -> String {
        to_json(&ClinkResult {
            success: true,
            output: self.storage.lino_lines().unwrap_or_default().join("\n"),
            error: None,
            links: self.storage.snapshot(),
        })
    }

    #[wasm_bindgen]
    pub fn reset(&mut self) -> String {
        self.storage = BrowserStorage::new();
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
            output.extend(self.storage.lino_lines()?);
        }

        if !query.trim().is_empty() {
            let processor = QueryProcessor::new(options.trace)
                .with_auto_create_missing_references(options.auto_create_missing_references);
            let changes = processor.process_query(&mut self.storage, query)?;
            if options.changes {
                for (before, after) in &changes {
                    output.push(self.storage.format_change(before, after)?);
                }
            }
        }

        if options.after {
            output.extend(self.storage.lino_lines()?);
        }

        Ok(self.result(output, true, None))
    }

    fn result(&self, output: Vec<String>, success: bool, error: Option<String>) -> ClinkResult {
        ClinkResult {
            success,
            output: output.join("\n"),
            error,
            links: self.storage.snapshot(),
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

#[derive(Default)]
struct BrowserStorage {
    links: HashMap<u32, Link>,
    names: HashMap<u32, String>,
    name_to_id: HashMap<String, u32>,
    next_id: u32,
}

impl BrowserStorage {
    fn new() -> Self {
        Self {
            next_id: 1,
            ..Self::default()
        }
    }

    fn snapshot(&self) -> Vec<WebLink> {
        let mut links: Vec<_> = self
            .links
            .values()
            .map(|link| WebLink {
                id: link.index,
                source: link.source,
                target: link.target,
                name: self.names.get(&link.index).cloned(),
            })
            .collect();
        links.sort_by_key(|link| link.id);
        links
    }

    fn format_change(&mut self, before: &Option<Link>, after: &Option<Link>) -> Result<String> {
        let before_text = before
            .map(|link| self.format_lino(&link))
            .transpose()?
            .unwrap_or_default();
        let after_text = after
            .map(|link| self.format_lino(&link))
            .transpose()?
            .unwrap_or_default();
        Ok(format!("({before_text}) ({after_text})"))
    }
}

impl NamedTypeLinks for BrowserStorage {
    fn create(&mut self, source: u32, target: u32) -> u32 {
        let id = self.next_id.max(1);
        self.next_id = id + 1;
        self.links.insert(id, Link::new(id, source, target));
        id
    }

    fn ensure_created(&mut self, id: u32) -> u32 {
        if id == 0 || self.links.contains_key(&id) {
            return id;
        }

        self.next_id = self.next_id.max(id + 1);
        self.links.insert(id, Link::new(id, 0, 0));
        id
    }

    fn get_link(&mut self, id: u32) -> Option<Link> {
        self.links.get(&id).copied()
    }

    fn exists(&mut self, id: u32) -> bool {
        self.links.contains_key(&id)
    }

    fn update(&mut self, id: u32, source: u32, target: u32) -> Result<Link> {
        let link = self.links.get_mut(&id).ok_or(LinkError::NotFound(id))?;
        let before = *link;
        link.source = source;
        link.target = target;
        Ok(before)
    }

    fn delete(&mut self, id: u32) -> Result<Link> {
        self.remove_name(id)?;
        self.links.remove(&id).ok_or(LinkError::NotFound(id).into())
    }

    fn all_links(&mut self) -> Vec<Link> {
        self.links.values().copied().collect()
    }

    fn search(&mut self, source: u32, target: u32) -> Option<u32> {
        self.links
            .values()
            .find(|link| link.source == source && link.target == target)
            .map(|link| link.index)
    }

    fn get_or_create(&mut self, source: u32, target: u32) -> u32 {
        self.search(source, target)
            .unwrap_or_else(|| self.create(source, target))
    }

    fn get_name(&mut self, id: u32) -> Result<Option<String>> {
        Ok(self.names.get(&id).cloned())
    }

    fn set_name(&mut self, id: u32, name: &str) -> Result<u32> {
        if let Some(previous_name) = self.names.remove(&id) {
            self.name_to_id.remove(&previous_name);
        }
        if let Some(previous_id) = self.name_to_id.insert(name.to_string(), id) {
            if previous_id != id {
                self.names.remove(&previous_id);
            }
        }
        self.names.insert(id, name.to_string());
        Ok(id)
    }

    fn get_by_name(&mut self, name: &str) -> Result<Option<u32>> {
        Ok(self.name_to_id.get(name).copied())
    }

    fn remove_name(&mut self, id: u32) -> Result<()> {
        if let Some(name) = self.names.remove(&id) {
            self.name_to_id.remove(&name);
        }
        Ok(())
    }

    fn save(&mut self) -> Result<()> {
        Ok(())
    }
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
