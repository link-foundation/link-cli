//! JS uses the native engine through WASM, rather than reimplementing isolation.
use crate::engine_log::{BindingLog, JsCommitLog};
use link_cli::engine::{
    Branch, ConcurrencyControl, Constraint, ConstraintValidation, Engine, EngineOptions,
    EngineTransaction, GroupCommitOutcome, HistoryPolicy, IsolationLevel, MemoryLog,
    RevisionEncoding, SerializableValidation, StateStructure,
};
use link_cli::protocol::{error_document, execute_request, format_document, parse_document};
use link_cli::{LinksStorage, QueryProcessor, TransitionLogStore};
use serde::Deserialize;
use wasm_bindgen::prelude::*;

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
struct Options {
    state_structure: Option<String>,
    revision_encoding: Option<String>,
    concurrency: Option<String>,
    isolation: Option<String>,
    serializable_validation: Option<String>,
    history: Option<serde_json::Value>,
    constraints: Vec<String>,
    constraint_validation: Option<String>,
    trace: bool,
    require_durable_flush: bool,
}
fn options(text: &str) -> Result<EngineOptions, String> {
    // Serde structs also accept positional arrays. The public binding accepts
    // a JSON options object, matching the JS client rather than silently
    // treating [] (or a positional policy array) as configuration.
    let object: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(text).map_err(|error| error.to_string())?;
    let parsed: Options = serde_json::from_value(serde_json::Value::Object(object))
        .map_err(|error| error.to_string())?;
    Ok(EngineOptions {
        constraint_validation: match parsed.constraint_validation.as_deref() {
            None | Some("scan") => ConstraintValidation::Scan,
            Some("indexed") => ConstraintValidation::Indexed,
            _ => return Err("unknown constraint validation".into()),
        },
        revision_encoding: match parsed.revision_encoding.as_deref() {
            None | Some("snapshot") => RevisionEncoding::Snapshot,
            Some("changes") => RevisionEncoding::Changes,
            _ => return Err("unknown revision encoding".into()),
        },
        state_structure: match parsed.state_structure.as_deref() {
            None | Some("flat") => StateStructure::Flat,
            Some("shared") => StateStructure::Shared,
            _ => return Err("unknown state structure".into()),
        },
        concurrency: match parsed.concurrency.as_deref() {
            None | Some("optimistic") => ConcurrencyControl::Optimistic,
            Some("exclusive") => ConcurrencyControl::Exclusive,
            Some("pessimistic") => ConcurrencyControl::Pessimistic,
            _ => return Err("unknown concurrency control".into()),
        },
        isolation: match parsed.isolation.as_deref() {
            None | Some("serializable") => IsolationLevel::Serializable,
            Some("snapshot") => IsolationLevel::Snapshot,
            Some("read-committed") => IsolationLevel::ReadCommitted,
            _ => return Err("unknown isolation level".into()),
        },
        serializable_validation: match parsed.serializable_validation.as_deref() {
            None | Some("revision") => SerializableValidation::Revision,
            Some("read-set") => SerializableValidation::ReadSet,
            _ => return Err("unknown serializable validation".into()),
        },
        history: match parsed.history {
            None | Some(serde_json::Value::Null) => HistoryPolicy::None,
            Some(serde_json::Value::String(text)) if text == "all" => HistoryPolicy::All,
            Some(serde_json::Value::Number(number)) => HistoryPolicy::Last(
                number
                    .as_u64()
                    .and_then(|n| usize::try_from(n).ok())
                    .filter(|n| *n > 0)
                    .ok_or("history must be positive")?,
            ),
            _ => return Err("history must be null, all, or a positive integer".into()),
        },
        constraints: parsed
            .constraints
            .iter()
            .map(|constraint| match constraint.as_str() {
                "references" => Ok(Constraint::References),
                "unique-pairs" => Ok(Constraint::UniquePairs),
                "unique-names" => Ok(Constraint::UniqueNames),
                _ => Err("unknown constraint".into()),
            })
            .collect::<Result<_, String>>()?,
        trace: parsed.trace,
        require_durable_flush: parsed.require_durable_flush,
    })
}

#[wasm_bindgen]
pub struct StagedEngine {
    inner: Engine<u32, BindingLog>,
}
#[wasm_bindgen]
impl StagedEngine {
    /// journal_json is an array of engine1/engine2 records ("[]" for an empty store).
    #[wasm_bindgen(constructor)]
    pub fn new(options_json: &str, journal_json: &str) -> Result<StagedEngine, String> {
        let entries = serde_json::from_str(journal_json).map_err(|error| error.to_string())?;
        Ok(Self {
            inner: Engine::open(
                BindingLog::Memory(MemoryLog { entries }),
                options(options_json)?,
            )
            .map_err(|e| e.to_string())?,
        })
    }
    /// The caller owns the external adapter's resources and fencing.
    #[wasm_bindgen(js_name = withLog)]
    pub fn with_log(options_json: &str, log: JsCommitLog) -> Result<StagedEngine, String> {
        Ok(Self {
            inner: Engine::open(BindingLog::External(log), options(options_json)?)
                .map_err(|e| e.to_string())?,
        })
    }
    pub fn begin(&self) -> Result<StagedSession, String> {
        Ok(StagedSession {
            inner: self.inner.begin().map_err(|e| e.to_string())?,
        })
    }
    /// Own the group handle before transferring sessions from their JS queues.
    pub fn group(&self) -> StagedCommitGroup {
        StagedCommitGroup {
            engine: self.inner.clone(),
            sessions: Vec::new(),
            closed: false,
        }
    }
    pub fn fork(&self, options_json: &str, journal_json: &str) -> Result<StagedBranch, String> {
        let entries = serde_json::from_str(journal_json).map_err(|error| error.to_string())?;
        self.fork_log(options_json, BindingLog::Memory(MemoryLog { entries }))
    }
    #[wasm_bindgen(js_name = forkWithLog)]
    pub fn fork_with_log(
        &self,
        options_json: &str,
        log: JsCommitLog,
    ) -> Result<StagedBranch, String> {
        self.fork_log(options_json, BindingLog::External(log))
    }
    pub fn resume(
        &self,
        options_json: &str,
        journal_json: &str,
        base: &str,
    ) -> Result<StagedBranch, String> {
        let entries = serde_json::from_str(journal_json).map_err(|error| error.to_string())?;
        self.resume_log(
            options_json,
            BindingLog::Memory(MemoryLog { entries }),
            base,
        )
    }
    #[wasm_bindgen(js_name = resumeWithLog)]
    pub fn resume_with_log(
        &self,
        options_json: &str,
        log: JsCommitLog,
        base: &str,
    ) -> Result<StagedBranch, String> {
        self.resume_log(options_json, BindingLog::External(log), base)
    }
    /// Export does not persist anything; caller owns the resulting bytes.
    pub fn journal(&self) -> Result<String, String> {
        self.inner
            .with_log(|log| {
                Ok(serde_json::to_string(&log.read_log_entries()?).expect("strings serialize"))
            })
            .map_err(|e| e.to_string())
    }
    /// Historical rows use decimal addresses, avoiding JS number precision loss.
    pub fn snapshot(&self, revision: Option<u64>) -> Result<String, String> {
        let snapshot = match revision {
            Some(revision) => self.inner.snapshot_at(revision),
            None => self.inner.snapshot(),
        }
        .map_err(|e| e.to_string())?;
        let rows: Vec<_> = snapshot.iter().map(|(_, row)| serde_json::json!({
            "index": row.link.index.to_string(), "source": row.link.source.to_string(), "target": row.link.target.to_string(), "name": row.name,
        })).collect();
        Ok(
            serde_json::json!({"revision": snapshot.revision.to_string(), "rows": rows})
                .to_string(),
        )
    }
}

#[wasm_bindgen]
pub struct StagedCommitGroup {
    engine: Engine<u32, BindingLog>,
    sessions: Vec<EngineTransaction<u32, BindingLog>>,
    closed: bool,
}
#[wasm_bindgen]
impl StagedCommitGroup {
    /// Consumes the handle; freeing an uncommitted group rolls back its members.
    pub fn add(&mut self, session: StagedSession) -> Result<(), String> {
        if self.closed {
            return Err("commit group is closed".into());
        }
        self.sessions.push(session.inner);
        Ok(())
    }
    pub fn commit(&mut self) -> Result<String, String> {
        if self.closed {
            return Err("commit group is closed".into());
        }
        self.closed = true;
        let outcomes = self
            .engine
            .commit_group(std::mem::take(&mut self.sessions))
            .map_err(|e| e.to_string())?;
        let values: Vec<_> = outcomes
            .into_iter()
            .map(|outcome| match outcome {
                GroupCommitOutcome::Committed(revision) => {
                    serde_json::json!({"status":"committed", "revision":revision.to_string()})
                }
                GroupCommitOutcome::Aborted(error) => {
                    serde_json::json!({"status":"aborted", "error":error})
                }
                GroupCommitOutcome::Indeterminate(error) => {
                    serde_json::json!({"status":"indeterminate", "error":error})
                }
            })
            .collect();
        Ok(serde_json::Value::Array(values).to_string())
    }
}

impl StagedEngine {
    fn fork_log(&self, options_json: &str, log: BindingLog) -> Result<StagedBranch, String> {
        Ok(StagedBranch {
            inner: self
                .inner
                .fork(log, options(options_json)?)
                .map_err(|error| error.to_string())?,
        })
    }
    fn resume_log(
        &self,
        options_json: &str,
        log: BindingLog,
        base: &str,
    ) -> Result<StagedBranch, String> {
        Ok(StagedBranch {
            inner: self
                .inner
                .resume_branch(log, options(options_json)?, base)
                .map_err(|error| error.to_string())?,
        })
    }
}

#[wasm_bindgen]
pub struct StagedBranch {
    inner: Branch<u32, BindingLog, BindingLog>,
}
#[wasm_bindgen]
impl StagedBranch {
    pub fn engine(&self) -> StagedEngine {
        StagedEngine {
            inner: self.inner.engine().clone(),
        }
    }
    pub fn merge(&mut self) -> Result<u64, String> {
        self.inner.merge().map_err(|error| error.to_string())
    }
    #[wasm_bindgen(js_name = mergeBaseRecord)]
    pub fn merge_base_record(&self) -> String {
        self.inner.merge_base_record()
    }
}

#[wasm_bindgen]
pub struct StagedSession {
    inner: EngineTransaction<u32, BindingLog>,
}
#[wasm_bindgen]
impl StagedSession {
    pub fn request(&mut self, text: &str) -> String {
        let reply = match parse_document(text) {
            Ok(document) => {
                execute_request(&mut self.inner, &QueryProcessor::new(false), &document)
            }
            Err(error) => error_document(&error.to_string()),
        };
        format_document(&reply)
    }
    pub fn commit(&mut self) -> Result<u64, String> {
        self.inner.commit().map_err(|e| e.to_string())
    }
    pub fn rollback(&mut self) {
        self.inner.rollback();
    }
    pub fn count(&self) -> usize {
        self.inner.links_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn binding_sessions_and_journal_round_trip() {
        let engine = StagedEngine::new(r#"{"history":"all"}"#, "[]").unwrap();
        let mut session = engine.begin().unwrap();
        assert!(!session.request("(create: (0 0))").contains("error:"));
        assert_eq!(engine.begin().unwrap().count(), 0);
        assert_eq!(session.commit().unwrap(), 1);
        assert!(session.request("(create: (0 0))").contains("error:"));
        let reopened = StagedEngine::new("{}", &engine.journal().unwrap()).unwrap();
        assert_eq!(reopened.begin().unwrap().count(), 1);
        assert!(engine.snapshot(Some(0)).unwrap().contains("\"rows\":[]"));
    }
    #[test]
    fn invalid_policies_are_not_silently_ignored() {
        for text in [
            "[]",
            r#"{"isolation":"dirty"}"#,
            r#"{"serializableValidation":"invalid"}"#,
            r#"{"serializableValidation":42}"#,
            r#"{"isolation":"snapshot","serializableValidation":"read-set"}"#,
            r#"{"concurrency":"none"}"#,
            r#"{"stateStructure":"mutable"}"#,
            r#"{"stateStructure":42}"#,
            r#"{"history":0}"#,
            r#"{"constraints":["unknown"]}"#,
            r#"{"constraintValidation":"unknown"}"#,
            r#"{"durable":true}"#,
        ] {
            assert!(StagedEngine::new(text, "[]").is_err(), "{text}");
        }
    }
}
