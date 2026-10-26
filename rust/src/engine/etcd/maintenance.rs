use super::*;
use crate::engine::maintenance::{
    JournalMaintenanceStore, JournalNamespaceEntry, JournalScanOptions,
};

fn count(value: &Value) -> Option<usize> {
    let text = value.as_str()?;
    if text == "0" {
        return Some(0);
    }
    usize::try_from(generation(text)?).ok()
}
fn decode(value: &Value) -> Result<String, LinkError> {
    let text = value
        .as_str()
        .ok_or_else(|| error("missing namespace base64"))?;
    let bytes = STANDARD
        .decode(text)
        .map_err(|_| error("invalid namespace base64"))?;
    if STANDARD.encode(&bytes) != text {
        return Err(error("noncanonical namespace base64"));
    }
    String::from_utf8(bytes).map_err(|_| error("invalid namespace UTF-8"))
}
impl EtcdJournalStore {
    fn scan_once(
        &self,
        options: JournalScanOptions,
    ) -> Result<Vec<JournalNamespaceEntry>, LinkError> {
        let mut entries = Vec::new();
        let mut bytes = 0usize;
        let mut revision: Option<u64> = None;
        let mut previous = self.prefix.clone();
        let mut start = previous.clone();
        // Prefix ends in ASCII '/', so incrementing that final byte gives its
        // exclusive lexical successor even when the preceding UTF-8 is non-ASCII.
        let end = format!("{}0", &self.prefix[..self.prefix.len() - 1]);
        loop {
            let limit = options.page_keys.min(
                options
                    .max_keys
                    .saturating_sub(entries.len())
                    .saturating_add(1),
            );
            let mut range = json!({"key":STANDARD.encode(&start),"range_end":STANDARD.encode(&end),"limit":limit.to_string(),"sort_order":"ASCEND","sort_target":"KEY","keys_only":options.keys_only});
            if let Some(revision) = revision {
                range["revision"] = json!(revision.to_string());
            }
            let response = self.post(json!({"success":[{"request_range":range}]}))?;
            let current = response["header"]["revision"]
                .as_str()
                .and_then(generation)
                .ok_or_else(|| error("invalid namespace revision"))?;
            let pinned = *revision.get_or_insert(current);
            let responses = response["responses"]
                .as_array()
                .ok_or_else(|| error("invalid namespace responses"))?;
            if current < pinned || response["succeeded"] != true || responses.len() != 1 {
                return Err(error("invalid namespace snapshot"));
            }
            let range = responses[0]["response_range"]
                .as_object()
                .ok_or_else(|| error("invalid namespace range"))?;
            let more = match range.get("more") {
                None => false,
                Some(value) => value
                    .as_bool()
                    .ok_or_else(|| error("invalid namespace continuation"))?,
            };
            let values = match range.get("kvs") {
                None => &[][..],
                Some(value) => value
                    .as_array()
                    .ok_or_else(|| error("invalid namespace keys"))?
                    .as_slice(),
            };
            let count = match range.get("count") {
                None => Some(0),
                Some(value) if !value.is_null() => count(value),
                _ => None,
            }
            .ok_or_else(|| error("invalid namespace count"))?;
            if values.len() > limit
                || (more && (values.is_empty() || count <= values.len()))
                || (!more && count != values.len())
            {
                return Err(error("incomplete namespace page"));
            }
            for value in values {
                let key = decode(&value["key"])?;
                let name = key
                    .strip_prefix(&self.prefix)
                    .ok_or_else(|| error("foreign namespace key"))?;
                validate_keys(&[name.into()])?;
                if key <= previous {
                    return Err(error("unordered namespace keys"));
                }
                let generation = value["mod_revision"]
                    .as_str()
                    .and_then(generation)
                    .filter(|generation| *generation <= pinned)
                    .ok_or_else(|| error("invalid namespace generation"))?;
                let records: Vec<String> = if options.keys_only {
                    if value.get("value").is_some_and(|value| value != "") {
                        return Err(error("unexpected namespace payload"));
                    }
                    Vec::new()
                } else {
                    serde_json::from_str(&decode(&value["value"])?)
                        .map_err(|_| error("invalid namespace journal"))?
                };
                let size = records.iter().try_fold(
                    name.len()
                        .checked_add(8)
                        .ok_or_else(|| error("namespace byte limit"))?,
                    |size, record| {
                        size.checked_add(record.len())
                            .and_then(|size| size.checked_add(1))
                            .ok_or_else(|| error("namespace byte limit"))
                    },
                )?;
                bytes = bytes
                    .checked_add(size)
                    .filter(|size| *size <= options.max_bytes)
                    .ok_or_else(|| error("namespace byte limit"))?;
                if entries.len() == options.max_keys {
                    return Err(error("namespace key limit"));
                }
                entries.push(JournalNamespaceEntry {
                    key: name.into(),
                    version: JournalVersion {
                        generation,
                        entries: records,
                    },
                });
                previous = key;
            }
            if !more {
                return Ok(entries);
            }
            if entries.len() == options.max_keys {
                return Err(error("namespace key limit"));
            }
            start = format!("{previous}\0");
        }
    }
}
impl JournalMaintenanceStore for EtcdJournalStore {
    fn scan_namespace(
        &mut self,
        options: JournalScanOptions,
    ) -> Result<Vec<JournalNamespaceEntry>, LinkError> {
        if options.max_keys == 0
            || options.max_bytes == 0
            || !(1..=256).contains(&options.page_keys)
        {
            return Err(error("invalid namespace scan limits"));
        }
        let mut failure = error("no namespace endpoint available");
        for _ in 0..self.endpoints.len() {
            match self.scan_once(options) {
                Ok(snapshot) => return Ok(snapshot),
                Err(cause) => {
                    failure = cause;
                    self.cursor = (self.cursor + 1) % self.endpoints.len();
                }
            }
        }
        Err(failure)
    }
    fn compare_delete(&mut self, key: &str, expected_generation: u64) -> Result<bool, LinkError> {
        validate_keys(&[key.into()])?;
        if expected_generation == 0 || expected_generation > i64::MAX as u64 {
            return Err(error("invalid deletion generation"));
        }
        let response = self.post(json!({"compare":[{"key":self.key(key),"target":"MOD","result":"EQUAL","mod_revision":expected_generation.to_string()}],"success":[{"request_delete_range":{"key":self.key(key)}}]}))?;
        if response["header"]["revision"]
            .as_str()
            .and_then(generation)
            .is_none()
        {
            return Err(error("indeterminate deletion response"));
        }
        let succeeded = match response.get("succeeded") {
            None => false,
            Some(value) => value
                .as_bool()
                .ok_or_else(|| error("indeterminate deletion response"))?,
        };
        let responses = match response.get("responses") {
            None => &[][..],
            Some(value) => value
                .as_array()
                .ok_or_else(|| error("indeterminate deletion response"))?
                .as_slice(),
        };
        if (!succeeded && !responses.is_empty())
            || (succeeded
                && (responses.len() != 1
                    || responses[0]["response_delete_range"]["deleted"] != "1"))
        {
            return Err(error("indeterminate deletion response"));
        }
        Ok(succeeded)
    }
}
