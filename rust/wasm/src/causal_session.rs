//! Host-independent u64 workspace; publication stays with the host batch adapter.
use link_cli::engine::{
    causal::CausalVersion,
    causal_projection::{CausalConflictPolicy, CausalProjectionOptions},
    causal_session::{CausalWorkspace, CausalWorkspaceOptions},
};
use link_cli::{GenericLink, LinksStorage};
use serde_json::{json, Value};
use wasm_bindgen::prelude::*;

fn number(value: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .ok()
        .filter(|id| id.to_string() == value)
        .ok_or_else(|| "expected a canonical u64 decimal string".into())
}
fn address(value: &Value) -> Result<u64, String> {
    number(value.as_str().ok_or("expected an address string")?)
}
fn link(value: GenericLink<u64>) -> Value {
    json!({"index": value.index.to_string(), "source": value.source.to_string(), "target": value.target.to_string()})
}
fn options(text: &str) -> Result<CausalWorkspaceOptions, String> {
    let mut object: serde_json::Map<String, Value> =
        serde_json::from_str(text).map_err(|e| e.to_string())?;
    if object.keys().any(|key| {
        ![
            "stateStructure",
            "constraintValidation",
            "constraints",
            "trace",
            "maxVersions",
            "maxBytes",
        ]
        .contains(&key.as_str())
    }) {
        return Err("unknown causal workspace option".into());
    }
    let mut projection = CausalProjectionOptions::default();
    if let Some(value) = object.remove("maxVersions") {
        projection.max_versions = value
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .ok_or("invalid maxVersions")?;
    }
    if let Some(value) = object.remove("maxBytes") {
        projection.max_bytes = value
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .ok_or("invalid maxBytes")?;
    }
    let ordinary = crate::engine::options(&Value::Object(object).to_string())?;
    Ok(CausalWorkspaceOptions {
        state_structure: ordinary.state_structure,
        constraints: ordinary.constraints,
        constraint_validation: ordinary.constraint_validation,
        trace: ordinary.trace,
        projection,
    })
}

#[wasm_bindgen]
pub struct CausalLinksWorkspace {
    inner: CausalWorkspace,
}
#[wasm_bindgen]
impl CausalLinksWorkspace {
    #[wasm_bindgen(constructor)]
    pub fn new(
        versions_json: &str,
        first: &str,
        last: &str,
        policy: &str,
        options_json: &str,
    ) -> Result<CausalLinksWorkspace, String> {
        if versions_json.len() > 16_785_410 {
            return Err("causal workspace input exceeds its byte bound".into());
        }
        let encoded: Vec<String> =
            serde_json::from_str(versions_json).map_err(|e| e.to_string())?;
        let versions = encoded
            .iter()
            .map(|v| CausalVersion::decode(v).map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        let policy = match policy {
            "reject" => CausalConflictPolicy::Reject,
            "max-dot" => CausalConflictPolicy::MaxDot,
            _ => return Err("unknown causal conflict policy".into()),
        };
        Ok(Self {
            inner: CausalWorkspace::new(
                &versions,
                number(first)?,
                number(last)?,
                policy,
                options(options_json)?,
            )
            .map_err(|e| e.to_string())?,
        })
    }
    /// Raw links/names and ordinary LiNo transport over the same u64 workspace.
    /// LiNo uses the checked wider query marker; publication remains explicit.
    pub fn request(&mut self, text: &str) -> Result<String, String> {
        if text.len() > 1_048_576 {
            return Err("causal workspace request exceeds its byte bound".into());
        }
        let request: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let parts = request.as_array().ok_or("expected an operation array")?;
        let result = match parts.as_slice() {
            [operation, text] if operation == "lino" => {
                use link_cli::protocol::{
                    error_document, execute_request_for, format_document,
                    parse_document_with_limits,
                };
                let text = text.as_str().ok_or("expected LiNo text")?;
                let reply = match parse_document_with_limits(
                    text,
                    link_cli::query_options::QueryLimits::causal(),
                ) {
                    Ok(document) => execute_request_for(
                        &mut self.inner.named_links(),
                        &link_cli::GenericQueryProcessor::<u128>::new(false),
                        &document,
                    ),
                    Err(error) => error_document(&error.to_string()),
                };
                json!(format_document(&reply))
            }
            [operation] if operation == "rows" => Value::Array(
                self.inner
                    .rows()
                    .iter()
                    .map(|row| {
                        let mut value = link(row.link);
                        value["name"] = json!(row.name);
                        value
                    })
                    .collect(),
            ),
            [operation, source, target] if operation == "create" => json!(self
                .inner
                .create_link(address(source)?, address(target)?)
                .map_err(|e| e.to_string())?
                .to_string()),
            [operation, id] if operation == "ensure" => json!(self
                .inner
                .ensure_link_created(address(id)?)
                .map_err(|e| e.to_string())?
                .to_string()),
            [operation, id] if operation == "get" => {
                self.inner.get_link(address(id)?).map_or(Value::Null, link)
            }
            [operation, id, source, target] if operation == "update" => link(
                self.inner
                    .update_link(address(id)?, address(source)?, address(target)?)
                    .map_err(|e| e.to_string())?,
            ),
            [operation, id] if operation == "delete" => link(
                self.inner
                    .delete_link(address(id)?)
                    .map_err(|e| e.to_string())?,
            ),
            [operation, id] if operation == "get-name" => json!(self.inner.get_name(address(id)?)),
            [operation, name] if operation == "get-by-name" => json!(self
                .inner
                .get_by_name(name.as_str().ok_or("expected a name")?)
                .map(|id| id.to_string())),
            [operation, id, name] if operation == "set-name" => {
                let name = if name.is_null() {
                    None
                } else {
                    Some(name.as_str().ok_or("expected a name or null")?.to_owned())
                };
                self.inner
                    .set_name(address(id)?, name)
                    .map_err(|e| e.to_string())?;
                Value::Null
            }
            _ => return Err("unknown causal workspace operation".into()),
        };
        Ok(result.to_string())
    }
    pub fn finish(&mut self) -> Result<String, String> {
        let edits = self.inner.finish().map_err(|e| e.to_string())?;
        let values = edits.iter().map(|edit| {
            let observed = edit.observed.iter().map(|v| v.encode().map_err(|e| e.to_string())).collect::<Result<Vec<_>, _>>()?;
            let payload = edit.payload.as_ref().map(|p| json!({"source": p.source.to_string(), "target": p.target.to_string(), "name": p.name}));
            Ok(json!({"address": edit.address.to_string(), "observed": observed, "payload": payload}))
        }).collect::<Result<Vec<_>, String>>()?;
        Ok(Value::Array(values).to_string())
    }
}
