use super::*;

pub(super) const EPOCH_KEY: &str = "__link_gc_epoch";
pub(super) fn keys(keys: &[String]) -> Result<(), LinkError> {
    validate_keys(keys)?;
    if keys.len() > 31 || keys.iter().any(|key| key == EPOCH_KEY) {
        return Err(error("select 1..31 nonreserved shards"));
    }
    Ok(())
}
pub(super) fn version(value: &JournalVersion, absent: bool) -> Result<(), LinkError> {
    if value.generation == 0 && (!absent || !value.entries.is_empty()) {
        return Err(error("invalid reclamation generation"));
    }
    Ok(())
}
pub(super) fn read<S: AtomicJournalStore>(
    store: &mut S,
    keys: &[String],
) -> Result<Vec<JournalVersion>, LinkError> {
    let result = store.read(keys)?;
    if result.len() != keys.len() {
        return Err(error("incomplete reclamation snapshot"));
    }
    for item in &result {
        version(item, true)?;
    }
    Ok(result)
}
pub(super) fn epoch(value: &JournalVersion) -> Result<u64, LinkError> {
    version(value, false)?;
    if value.entries.len() != 2 || value.entries[0] != "gc1" {
        return Err(error("invalid reclamation epoch"));
    }
    canonical(&value.entries[1]).ok_or_else(|| error("invalid reclamation epoch"))
}
fn canonical(text: &str) -> Option<u64> {
    let value: u64 = text.parse().ok()?;
    (value.to_string() == text).then_some(value)
}
pub(super) fn key_epoch(key: &str) -> Result<u64, LinkError> {
    let bytes = key.as_bytes();
    if bytes.len() != 39
        || !bytes.starts_with(b"blob2-")
        || bytes[22] != b'-'
        || !bytes[6..22]
            .iter()
            .chain(&bytes[23..])
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
    {
        return Err(error("invalid reclamation object key"));
    }
    u64::from_str_radix(&key[6..22], 16).map_err(|_| error("invalid reclamation object epoch"))
}
pub(super) fn address(epoch: u64, records: &[String]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    let mut add = |bytes: &[u8]| {
        for byte in bytes {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
        }
    };
    add(b"blob2|");
    for record in records {
        add(format!("{}:", record.len()).as_bytes());
        add(record.as_bytes());
    }
    format!("blob2-{epoch:016x}-{hash:016x}")
}
pub(super) struct Pointer {
    pub partition: String,
    pub references: Vec<Reference>,
}
pub(super) struct Reference {
    pub key: String,
    pub generation: u64,
    pub epoch: u64,
    pub records: Option<usize>,
}
fn reference(
    key: &str,
    text: &str,
    records: Option<usize>,
    epoch: u64,
) -> Result<Reference, LinkError> {
    let generation = canonical(text)
        .filter(|value| *value > 0)
        .ok_or_else(|| error("invalid reclamation object generation"))?;
    let object_epoch = key_epoch(key)?;
    if object_epoch > epoch {
        return Err(error("pointer epoch exceeds catalog epoch"));
    }
    Ok(Reference {
        key: key.into(),
        generation,
        epoch: object_epoch,
        records,
    })
}
pub(super) fn pointer<D>(
    value: &JournalVersion,
    epoch: u64,
    partitions: &BTreeMap<String, D>,
) -> Result<Option<Pointer>, LinkError> {
    version(value, true)?;
    if value.generation == 0 {
        return Ok(None);
    }
    let parts = &value.entries;
    if parts.len() < 4 || !partitions.contains_key(&parts[1]) {
        return Err(error("invalid reclamation pointer"));
    }
    let references = if parts.len() == 4 && parts[0] == "blob2" {
        vec![reference(&parts[2], &parts[3], None, epoch)?]
    } else {
        let invalid = || error("invalid reclamation chunk manifest");
        let total = canonical(&parts[2])
            .filter(|n| *n <= u32::MAX.into())
            .ok_or_else(invalid)?;
        let count = canonical(&parts[3])
            .filter(|n| *n <= 4096)
            .ok_or_else(invalid)? as usize;
        if parts[0] != "chunks2" || parts.len() != 4 + 3 * count {
            return Err(invalid());
        }
        let mut references = Vec::with_capacity(count);
        let mut sum = 0;
        for triple in parts[4..].as_chunks::<3>().0 {
            let records = canonical(&triple[2])
                .filter(|n| (1..=65536).contains(n))
                .ok_or_else(invalid)?;
            sum += records;
            references.push(reference(
                &triple[0],
                &triple[1],
                Some(records as usize),
                epoch,
            )?);
        }
        if sum != total {
            return Err(invalid());
        }
        references
    };
    Ok(Some(Pointer {
        partition: parts[1].clone(),
        references,
    }))
}
pub(super) fn scan<S: JournalMaintenanceStore>(
    store: &mut S,
    options: ReclamationOptions,
    keys_only: bool,
) -> Result<Vec<JournalNamespaceEntry>, LinkError> {
    let entries = store.scan_namespace(JournalScanOptions {
        max_keys: options.max_keys,
        max_bytes: options.max_bytes,
        page_keys: options.page_keys,
        keys_only,
    })?;
    if entries.len() > options.max_keys {
        return Err(error("reclamation scan key limit"));
    }
    let mut previous = "";
    let mut bytes = 0usize;
    for entry in &entries {
        validate_keys(std::slice::from_ref(&entry.key))?;
        if entry.key.as_str() <= previous {
            return Err(error("unordered reclamation scan"));
        }
        version(&entry.version, false)?;
        if keys_only && !entry.version.entries.is_empty() {
            return Err(error("unexpected keys-only records"));
        }
        let size = entry
            .version
            .entries
            .iter()
            .fold(entry.key.len().checked_add(8), |size, record| {
                size?.checked_add(record.len())?.checked_add(1)
            });
        bytes = size
            .and_then(|size| bytes.checked_add(size))
            .filter(|size| *size <= options.max_bytes)
            .ok_or_else(|| error("reclamation scan byte limit"))?;
        previous = &entry.key;
    }
    Ok(entries)
}
pub(super) type Protected = BTreeMap<(String, String), u64>;
pub(super) fn catalog<D>(
    entries: &[JournalNamespaceEntry],
    partitions: &BTreeMap<String, D>,
) -> Result<(u64, Protected), LinkError> {
    let root = entries
        .iter()
        .find(|item| item.key == EPOCH_KEY)
        .ok_or_else(|| error("missing reclamation epoch"))?;
    let epoch = epoch(&root.version)?;
    let mut protected = Protected::new();
    for item in entries {
        if item.key == EPOCH_KEY {
            continue;
        }
        let pointer = pointer(&item.version, epoch, partitions)?
            .ok_or_else(|| error("absent catalog entry"))?;
        for reference in pointer.references {
            if let Some(prior) = protected.insert(
                (pointer.partition.clone(), reference.key),
                reference.generation,
            ) {
                if prior != reference.generation {
                    return Err(error("contradictory protected generations"));
                }
            }
        }
    }
    Ok((epoch, protected))
}
