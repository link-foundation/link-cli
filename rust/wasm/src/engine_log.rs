use link_cli::{engine::MemoryLog, LinkError, ReplaceableTransitionLogStore, TransitionLogStore};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const LOG_TYPES: &str = r#"
export interface CommitLog {
    readonly supportsDurableFlush: boolean;
    readonly supportsReplacement?: boolean;
    replace?(records: string[]): boolean;
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
    #[wasm_bindgen(method, getter, catch, js_name = supportsReplacement)]
    fn replaceable(this: &JsCommitLog) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(method, catch, js_name = replace)]
    fn replace(this: &JsCommitLog, records: &js_sys::Array) -> Result<JsValue, JsValue>;
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

impl ReplaceableTransitionLogStore for BindingLog {
    fn supports_replacement(&self) -> bool {
        match self {
            Self::Memory(_) => false,
            Self::External(log) => {
                log.replaceable().ok().and_then(|value| value.as_bool()) == Some(true)
            }
        }
    }
    fn replace_log_entries(&mut self, entries: &[String]) -> Result<bool, LinkError> {
        match self {
            Self::Memory(_) => Err(LinkError::Transaction(
                "memory log does not support journal replacement".into(),
            )),
            Self::External(log) => {
                let records = entries
                    .iter()
                    .map(|entry| JsValue::from_str(entry))
                    .collect::<js_sys::Array>();
                log.replace(&records)
                    .map_err(error)?
                    .as_bool()
                    .ok_or_else(|| {
                        LinkError::Transaction(
                            "log replacement must complete synchronously and return boolean".into(),
                        )
                    })
            }
        }
    }
}

#[cfg(all(test, target_arch = "wasm32"))]
mod tests {
    use super::*;
    use wasm_bindgen::JsCast;

    #[wasm_bindgen(inline_js = r#"
export function replacement_fixture(result) {
    return { supportsReplacement: true, records: [], replace(records) {
        if (!Array.isArray(records) || records.some(record => typeof record !== 'string'))
            throw 'expected string records';
        this.records = records;
        return result;
    }};
}
export function replacement_records(log) { return JSON.stringify(log.records); }
"#)]
    extern "C" {
        fn replacement_fixture(result: JsValue) -> JsValue;
        fn replacement_records(log: &JsValue) -> String;
    }

    #[wasm_bindgen_test::wasm_bindgen_test]
    fn external_replacement_trait_preserves_boolean_results_and_rejects_invalid_acknowledgement() {
        for result in [JsValue::TRUE, JsValue::FALSE, JsValue::UNDEFINED] {
            let raw = replacement_fixture(result.clone());
            let mut log = BindingLog::External(raw.clone().unchecked_into());
            assert!(log.supports_replacement());
            let outcome = log.replace_log_entries(&["one".into(), "two".into()]);
            if let Some(expected) = result.as_bool() {
                assert_eq!(outcome.unwrap(), expected);
            } else {
                assert!(outcome.unwrap_err().to_string().contains("synchronously"));
            }
            assert_eq!(replacement_records(&raw), r#"["one","two"]"#);
        }
    }

    #[wasm_bindgen_test::wasm_bindgen_test]
    fn memory_replacement_trait_rejects_without_mutation_and_keeps_ordinary_io() {
        let mut log = BindingLog::Memory(MemoryLog::default());
        log.append_log_entry("existing").unwrap();
        log.flush_log().unwrap();
        assert!(!log.supports_replacement());
        assert!(log.replace_log_entries(&["replacement".into()]).is_err());
        assert_eq!(log.read_log_entries().unwrap(), ["existing"]);
        log.append_log_entry("following").unwrap();
        log.flush_log().unwrap();
        assert_eq!(log.read_log_entries().unwrap(), ["existing", "following"]);
    }
}
