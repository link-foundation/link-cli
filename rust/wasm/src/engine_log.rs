use link_cli::{engine::MemoryLog, LinkError, TransitionLogStore};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const LOG_TYPES: &str = r#"
export interface CommitLog {
    readonly supportsDurableFlush: boolean;
    read(): string;
    append(record: string): void;
    flush(): void;
}
"#;

#[wasm_bindgen]
extern "C" {
    /// Synchronous adapter. read returns a JSON array of records. append and
    /// flush return undefined only after completing; Promises are rejected.
    #[wasm_bindgen(typescript_type = "CommitLog")]
    pub type JsCommitLog;
    #[wasm_bindgen(method, catch, js_name = read)]
    fn read(this: &JsCommitLog) -> Result<String, JsValue>;
    #[wasm_bindgen(method, catch, js_name = append)]
    fn append(this: &JsCommitLog, record: &str) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(method, catch, js_name = flush)]
    fn flush(this: &JsCommitLog) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(method, getter, catch, js_name = supportsDurableFlush)]
    fn durable(this: &JsCommitLog) -> Result<JsValue, JsValue>;
}

pub enum BindingLog {
    Memory(MemoryLog),
    External(JsCommitLog),
}
fn error(value: JsValue) -> LinkError {
    LinkError::Transaction(
        value
            .as_string()
            .unwrap_or_else(|| "JavaScript log adapter failed".into()),
    )
}
fn completed(value: JsValue) -> Result<(), LinkError> {
    if value.is_undefined() {
        Ok(())
    } else {
        Err(LinkError::Transaction(
            "log append/flush must complete synchronously and return undefined".into(),
        ))
    }
}
impl TransitionLogStore for BindingLog {
    fn append_log_entry(&mut self, entry: &str) -> Result<(), LinkError> {
        match self {
            Self::Memory(log) => log.append_log_entry(entry),
            Self::External(log) => completed(log.append(entry).map_err(error)?),
        }
    }
    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        match self {
            Self::Memory(log) => log.read_log_entries(),
            Self::External(log) => serde_json::from_str(&log.read().map_err(error)?)
                .map_err(|cause| LinkError::Transaction(format!("invalid log records: {cause}"))),
        }
    }
    fn flush_log(&mut self) -> Result<(), LinkError> {
        match self {
            Self::Memory(log) => log.flush_log(),
            Self::External(log) => completed(log.flush().map_err(error)?),
        }
    }
    fn supports_durable_flush(&self) -> bool {
        match self {
            Self::Memory(_) => false,
            Self::External(log) => {
                log.durable().ok().and_then(|value| value.as_bool()) == Some(true)
            }
        }
    }
}
