//! HTTP(S) etcd JSON gateway adapter. Requires a trusted durable etcd deployment.
//! One atomic Txn reads all selected keys; writes compare modification revisions.
use super::{
    cluster::{validate_keys, AtomicJournalStore, JournalVersion},
    error,
};
use crate::LinkError;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::{json, Value};
use std::{io::Read, time::Duration};

fn generation(value: &str) -> Option<u64> {
    if value.is_empty()
        || value.starts_with('0')
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    value
        .parse()
        .ok()
        .filter(|number| *number <= i64::MAX as u64)
}

pub struct EtcdJournalStore {
    endpoints: Vec<String>,
    prefix: String,
    agent: ureq::Agent,
    cursor: usize,
    pub headers: Vec<(String, String)>,
    pub max_request_bytes: usize,
    pub max_response_bytes: usize,
}
impl EtcdJournalStore {
    pub fn new(endpoints: Vec<String>, prefix: String) -> Result<Self, LinkError> {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(10)))
            .max_redirects(0)
            .build()
            .into();
        Self::with_agent(endpoints, prefix, agent)
    }
    /// Supply an agent to configure TLS trust, proxies and finite network deadlines.
    pub fn with_agent(
        endpoints: Vec<String>,
        prefix: String,
        agent: ureq::Agent,
    ) -> Result<Self, LinkError> {
        if endpoints.is_empty() || prefix.len() < 2 || !prefix.ends_with('/') {
            return Err(error("provide endpoints and a nonempty prefix ending in /"));
        }
        for endpoint in &endpoints {
            let uri = endpoint
                .parse::<ureq::http::Uri>()
                .map_err(|_| error("invalid etcd endpoint"))?;
            if !matches!(uri.scheme_str(), Some("http" | "https"))
                || uri.authority().is_none()
                || uri.query().is_some()
            {
                return Err(error("etcd endpoints must be HTTP(S) URLs"));
            }
        }
        Ok(Self {
            endpoints: endpoints
                .into_iter()
                .map(|url| url.trim_end_matches('/').into())
                .collect(),
            prefix,
            agent,
            cursor: 0,
            headers: Vec::new(),
            max_request_bytes: 1_048_576,
            max_response_bytes: 8_388_608,
        })
    }
    fn key(&self, key: &str) -> String {
        STANDARD.encode(format!("{}{key}", self.prefix))
    }
    fn post(&self, body: Value) -> Result<Value, LinkError> {
        let body = serde_json::to_vec(&body).map_err(|cause| error(&cause.to_string()))?;
        if body.len() > self.max_request_bytes {
            return Err(error("coordinator request exceeds byte limit"));
        }
        let mut request = self
            .agent
            .post(format!("{}/v3/kv/txn", self.endpoints[self.cursor]))
            .header("Content-Type", "application/json");
        for (name, value) in &self.headers {
            request = request.header(name.as_str(), value.as_str());
        }
        let mut response = request
            .send(body)
            .map_err(|cause| error(&format!("coordinator request failed: {cause}")))?;
        let mut bytes = Vec::new();
        response
            .body_mut()
            .as_reader()
            .take(self.max_response_bytes.saturating_add(1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|cause| error(&format!("coordinator response failed: {cause}")))?;
        if bytes.len() > self.max_response_bytes {
            return Err(error("coordinator response exceeds byte limit"));
        }
        serde_json::from_slice(&bytes)
            .map_err(|cause| error(&format!("invalid coordinator response: {cause}")))
    }
    fn read_once(&self, keys: &[String]) -> Result<Vec<JournalVersion>, LinkError> {
        let result = self.post(json!({ "success": keys.iter().map(|key| json!({ "request_range": { "key": self.key(key) } })).collect::<Vec<_>>() }))?;
        let responses = result["responses"]
            .as_array()
            .ok_or_else(|| error("incomplete coordinator snapshot"))?;
        if result["succeeded"] != true || responses.len() != keys.len() {
            return Err(error("invalid coordinator snapshot"));
        }
        responses
            .iter()
            .zip(keys)
            .map(|(response, key)| {
                let range = response["response_range"]
                    .as_object()
                    .ok_or_else(|| error("invalid coordinator range"))?;
                if range.get("more").is_some_and(|more| more != false) {
                    return Err(error("incomplete coordinator range"));
                }
                let values = match range.get("kvs") {
                    None => &[][..],
                    Some(value) => value
                        .as_array()
                        .ok_or_else(|| error("invalid coordinator keys"))?
                        .as_slice(),
                };
                if values.is_empty() {
                    return Ok(JournalVersion::default());
                }
                let value = &values[0];
                let generation = value["mod_revision"]
                    .as_str()
                    .and_then(generation)
                    .filter(|generation| *generation > 0 && *generation <= i64::MAX as u64)
                    .ok_or_else(|| error("invalid coordinator generation"))?;
                if values.len() != 1 || value["key"] != self.key(key) {
                    return Err(error("invalid coordinator key"));
                }
                let bytes = STANDARD
                    .decode(
                        value["value"]
                            .as_str()
                            .ok_or_else(|| error("missing coordinator journal"))?,
                    )
                    .map_err(|_| error("invalid coordinator base64"))?;
                let entries = serde_json::from_slice(&bytes)
                    .map_err(|_| error("invalid coordinator journal"))?;
                Ok(JournalVersion {
                    generation,
                    entries,
                })
            })
            .collect()
    }
}
impl AtomicJournalStore for EtcdJournalStore {
    fn supports_durable_flush(&self) -> bool {
        true
    }
    fn read(&mut self, keys: &[String]) -> Result<Vec<JournalVersion>, LinkError> {
        validate_keys(keys)?;
        let mut failure = error("no etcd endpoint available");
        for _ in 0..self.endpoints.len() {
            match self.read_once(keys) {
                Ok(snapshot) => return Ok(snapshot),
                Err(cause) => {
                    failure = cause;
                    self.cursor = (self.cursor + 1) % self.endpoints.len();
                }
            }
        }
        Err(failure)
    }
    fn compare_exchange(
        &mut self,
        keys: &[String],
        expected: &[JournalVersion],
        updates: &[Option<Vec<String>>],
    ) -> Result<bool, LinkError> {
        validate_keys(keys)?;
        if expected.len() != keys.len()
            || updates.len() != keys.len()
            || expected
                .iter()
                .any(|version| version.generation > i64::MAX as u64)
        {
            return Err(error("invalid coordinator compare arrays"));
        }
        let compare: Vec<_> = keys.iter().zip(expected).map(|(key, version)| json!({ "key": self.key(key), "target": "MOD", "result": "EQUAL", "mod_revision": version.generation.to_string() })).collect();
        let mut success = Vec::new();
        for (key, entries) in keys.iter().zip(updates) {
            if let Some(entries) = entries {
                success.push(json!({ "request_put": { "key": self.key(key), "value": STANDARD.encode(serde_json::to_vec(entries).map_err(|cause| error(&cause.to_string()))?) } }));
            }
        }
        // A mutation is deliberately sent once. A retry could turn its own
        // successful-but-unacknowledged CAS into an apparent conflict.
        let result = self.post(json!({ "compare": compare, "success": success }))?;
        if result["header"]["revision"]
            .as_str()
            .and_then(generation)
            .filter(|revision| *revision > 0 && *revision <= i64::MAX as u64)
            .is_none()
        {
            return Err(error("indeterminate coordinator response"));
        }
        // ProtoJSON omits the default false field on a rejected comparison.
        match result.get("succeeded") {
            None => Ok(false),
            Some(value) => value
                .as_bool()
                .ok_or_else(|| error("indeterminate coordinator response")),
        }
    }
}
