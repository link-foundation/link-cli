//! Selected bounded content snapshots and proof-checked causal transfer.
use super::{
    causal::{CausalDot, CausalVersion},
    causal_replica::CausalReplica,
    cluster::AtomicJournalStore,
};
use crate::LinkError;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

const DOMAIN: &[u8] = b"causal-merkle1\0";
type Hash = [u8; 32];
fn invalid() -> LinkError {
    LinkError::InvalidFormat("invalid causal Merkle state".into())
}
fn digest(parts: &[&[u8]]) -> Hash {
    let mut value = Sha256::new();
    for part in parts {
        value.update(part);
    }
    value.finalize().into()
}
fn branch(depth: usize, children: &[Hash; 16]) -> Hash {
    let mut value = Sha256::new();
    value.update(DOMAIN);
    value.update([b'N', depth as u8]);
    for child in children {
        value.update(child);
    }
    value.finalize().into()
}
fn empty() -> &'static [Hash; 17] {
    static VALUES: OnceLock<[Hash; 17]> = OnceLock::new();
    VALUES.get_or_init(|| {
        let mut values = [[0u8; 32]; 17];
        values[16] = digest(&[DOMAIN, b"E"]);
        for depth in (0..16).rev() {
            values[depth] = branch(depth, &[values[depth + 1]; 16]);
        }
        values
    })
}
fn hex(value: &Hash) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn hash(value: &str) -> Result<Hash, LinkError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid());
    }
    let mut result = [0u8; 32];
    for (i, item) in result.iter_mut().enumerate() {
        *item = u8::from_str_radix(&value[i * 2..i * 2 + 2], 16).map_err(|_| invalid())?;
    }
    Ok(result)
}
fn prefix(value: &str) -> Result<(), LinkError> {
    if value.len() > 15
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid());
    }
    Ok(())
}
fn admission(
    values: &[CausalVersion],
    max_versions: usize,
    max_bytes: usize,
) -> Result<BTreeMap<u64, Vec<CausalVersion>>, LinkError> {
    if values.len() > max_versions {
        return Err(invalid());
    }
    let mut bytes = 0usize;
    let mut dots: BTreeMap<&CausalDot, String> = BTreeMap::new();
    let mut groups: BTreeMap<u64, Vec<CausalVersion>> = BTreeMap::new();
    for value in values {
        let wire = value.encode()?;
        bytes = bytes.checked_add(wire.len()).ok_or_else(invalid)?;
        if bytes > max_bytes {
            return Err(invalid());
        }
        if dots.get(&value.dot).is_some_and(|prior| prior != &wire) {
            return Err(invalid());
        }
        dots.insert(&value.dot, wire);
        let group = groups.entry(value.address).or_default();
        group.push(value.clone());
        if group.len() > 64 {
            return Err(invalid());
        }
    }
    Ok(groups)
}
fn leaf_hash(address: u64, versions: &[CausalVersion]) -> Result<Hash, LinkError> {
    if versions.is_empty() {
        return Ok(empty()[16]);
    }
    let mut hash = Sha256::new();
    hash.update(DOMAIN);
    hash.update(b"L");
    hash.update(address.to_be_bytes());
    hash.update((versions.len() as u16).to_be_bytes());
    for value in versions {
        let wire = value.encode()?;
        hash.update((wire.len() as u32).to_be_bytes());
        hash.update(wire.as_bytes());
    }
    Ok(hash.finalize().into())
}
fn proof(value: &[Vec<String>], depth: usize) -> Result<Vec<Vec<Hash>>, LinkError> {
    if value.len() != depth {
        return Err(invalid());
    }
    value
        .iter()
        .map(|level| {
            if level.len() != 15 {
                return Err(invalid());
            }
            level.iter().map(|value| hash(value)).collect()
        })
        .collect()
}
fn fold(mut current: Hash, path: &str, proof: &[Vec<Hash>]) -> Hash {
    for depth in (0..path.len()).rev() {
        let digit = (path.as_bytes()[depth] as char).to_digit(16).unwrap() as usize;
        let mut children = [[0u8; 32]; 16];
        let mut index = 0;
        for (i, child) in children.iter_mut().enumerate() {
            if i == digit {
                *child = current;
            } else {
                *child = proof[depth][index];
                index += 1;
            }
        }
        current = branch(depth, &children);
    }
    current
}
fn frame(wire: &str, max_bytes: usize, tag: &str) -> Result<Value, LinkError> {
    if wire.len() > max_bytes {
        return Err(invalid());
    }
    let (mut depth, mut separators, mut quoted) = (0usize, 0usize, false);
    for byte in wire.bytes() {
        if !(b' '..=b'~').contains(&byte) || byte == b'\\' {
            return Err(invalid());
        }
        if byte == b'"' {
            quoted = !quoted;
            continue;
        }
        if quoted {
            continue;
        }
        match byte {
            b'{' | b'}' => return Err(invalid()),
            b'[' => {
                depth += 1;
                if depth > 5 {
                    return Err(invalid());
                }
            }
            b']' => {
                depth = depth.checked_sub(1).ok_or_else(invalid)?;
            }
            b',' => {
                separators += 1;
                if separators > 32768 {
                    return Err(invalid());
                }
            }
            _ => (),
        }
    }
    if quoted || depth != 0 {
        return Err(invalid());
    }
    let value: Value = serde_json::from_str(wire).map_err(|_| invalid())?;
    // Value's PartialEq<str> compares a JSON string value, not its wire encoding.
    let canonical = value.to_string();
    if !value
        .as_array()
        .is_some_and(|a| a.len() == 5 && a[0] == tag)
        || canonical != wire
    {
        return Err(invalid());
    }
    Ok(value)
}
fn text(value: &Value) -> Result<&str, LinkError> {
    value.as_str().ok_or_else(invalid)
}
fn strings(value: &Value, count: usize) -> Result<Vec<String>, LinkError> {
    value
        .as_array()
        .filter(|a| a.len() == count)
        .ok_or_else(invalid)?
        .iter()
        .map(|v| text(v).map(String::from))
        .collect()
}
fn wire_proof(value: &Value, depth: usize) -> Result<Vec<Vec<String>>, LinkError> {
    value
        .as_array()
        .filter(|a| a.len() == depth)
        .ok_or_else(invalid)?
        .iter()
        .map(|v| strings(v, 15))
        .collect()
}

#[derive(Clone, Copy, Debug)]
pub struct CausalMerkleOptions {
    pub max_versions: usize,
    pub max_bytes: usize,
}
impl Default for CausalMerkleOptions {
    fn default() -> Self {
        Self {
            max_versions: 4096,
            max_bytes: 8_388_608,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CausalMerkleNode {
    pub root: String,
    pub prefix: String,
    pub children: Vec<String>,
    pub proof: Vec<Vec<String>>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CausalMerkleLeaf {
    pub root: String,
    pub address: u64,
    pub versions: Vec<CausalVersion>,
    pub proof: Vec<Vec<String>>,
}
pub struct CausalMerkleSnapshot {
    root: String,
    rows: BTreeMap<u64, Vec<CausalVersion>>,
    nodes: BTreeMap<String, Hash>,
}
impl CausalMerkleSnapshot {
    pub fn new(values: &[CausalVersion], options: CausalMerkleOptions) -> Result<Self, LinkError> {
        if !(1..=4096).contains(&options.max_versions)
            || !(1..=8_388_608).contains(&options.max_bytes)
        {
            return Err(invalid());
        }
        let groups = admission(values, options.max_versions, options.max_bytes)?;
        let mut result = Self {
            root: String::new(),
            rows: BTreeMap::new(),
            nodes: BTreeMap::new(),
        };
        for (key, group) in groups {
            let merged = CausalVersion::merge(&group, &[])?;
            result
                .nodes
                .insert(format!("{key:016x}"), leaf_hash(key, &merged)?);
            result.rows.insert(key, merged);
        }
        for depth in (0..16).rev() {
            let parents: BTreeSet<String> = result
                .nodes
                .keys()
                .filter(|p| p.len() == depth + 1)
                .map(|p| p[..depth].into())
                .collect();
            for parent in parents {
                result
                    .nodes
                    .insert(parent.clone(), branch(depth, &result.children(&parent)));
            }
        }
        result.root = hex(result.nodes.get("").unwrap_or(&empty()[0]));
        Ok(result)
    }
    pub fn root(&self) -> &str {
        &self.root
    }
    fn children(&self, path: &str) -> [Hash; 16] {
        std::array::from_fn(|i| {
            *self
                .nodes
                .get(&format!("{path}{i:x}"))
                .unwrap_or(&empty()[path.len() + 1])
        })
    }
    fn proof(&self, path: &str) -> Vec<Vec<String>> {
        path.bytes()
            .enumerate()
            .map(|(depth, digit)| {
                let digit = (digit as char).to_digit(16).unwrap() as usize;
                self.children(&path[..depth])
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != digit)
                    .map(|(_, value)| hex(value))
                    .collect()
            })
            .collect()
    }
    pub fn node(&self, path: &str) -> Result<CausalMerkleNode, LinkError> {
        prefix(path)?;
        Ok(CausalMerkleNode {
            root: self.root.clone(),
            prefix: path.into(),
            children: self.children(path).iter().map(hex).collect(),
            proof: self.proof(path),
        })
    }
    pub fn leaf(&self, address: u64) -> Result<CausalMerkleLeaf, LinkError> {
        if address == 0 {
            return Err(invalid());
        }
        let path = format!("{address:016x}");
        Ok(CausalMerkleLeaf {
            root: self.root.clone(),
            address,
            versions: self.rows.get(&address).cloned().unwrap_or_default(),
            proof: self.proof(&path),
        })
    }
    pub fn verify_node(root: &str, value: &CausalMerkleNode) -> Result<Vec<String>, LinkError> {
        let expected = hash(root)?;
        prefix(&value.prefix)?;
        if value.root != root || value.children.len() != 16 {
            return Err(invalid());
        }
        let children: Vec<_> = value
            .children
            .iter()
            .map(|value| hash(value))
            .collect::<Result<_, _>>()?;
        let proof = proof(&value.proof, value.prefix.len())?;
        let children: [Hash; 16] = children.try_into().map_err(|_| invalid())?;
        if fold(branch(value.prefix.len(), &children), &value.prefix, &proof) != expected {
            return Err(invalid());
        }
        Ok(value.children.clone())
    }
    pub fn verify_leaf(
        root: &str,
        value: &CausalMerkleLeaf,
    ) -> Result<Vec<CausalVersion>, LinkError> {
        let expected = hash(root)?;
        if value.root != root || value.address == 0 {
            return Err(invalid());
        }
        admission(&value.versions, 64, 1_048_576)?;
        let proof = proof(&value.proof, 16)?;
        if value.versions.iter().any(|v| v.address != value.address)
            || CausalVersion::merge(&value.versions, &[])? != value.versions
        {
            return Err(invalid());
        }
        let current = leaf_hash(value.address, &value.versions)?;
        if fold(current, &format!("{:016x}", value.address), &proof) != expected {
            return Err(invalid());
        }
        Ok(value.versions.clone())
    }
    pub fn encode_node(value: &CausalMerkleNode) -> Result<String, LinkError> {
        Self::verify_node(&value.root, value)?;
        Ok(json!([
            "causal-node1",
            value.root,
            value.prefix,
            value.children,
            value.proof
        ])
        .to_string())
    }
    pub fn decode_node(root: &str, wire: &str) -> Result<CausalMerkleNode, LinkError> {
        let value = frame(wire, 32768, "causal-node1")?;
        let path = text(&value[2])?;
        prefix(path)?;
        let result = CausalMerkleNode {
            root: text(&value[1])?.into(),
            prefix: path.into(),
            children: strings(&value[3], 16)?,
            proof: wire_proof(&value[4], path.len())?,
        };
        Self::verify_node(root, &result)?;
        Ok(result)
    }
    pub fn encode_leaf(value: &CausalMerkleLeaf) -> Result<String, LinkError> {
        let versions = Self::verify_leaf(&value.root, value)?
            .iter()
            .map(|v| serde_json::from_str::<Value>(&v.encode()?).map_err(|_| invalid()))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(json!([
            "causal-leaf1",
            value.root,
            value.address.to_string(),
            versions,
            value.proof
        ])
        .to_string())
    }
    pub fn decode_leaf(root: &str, wire: &str) -> Result<CausalMerkleLeaf, LinkError> {
        let value = frame(wire, 1_081_344, "causal-leaf1")?;
        let key = text(&value[2])?;
        if key.is_empty() || key.len() > 20 {
            return Err(invalid());
        }
        let address = key.parse::<u64>().map_err(|_| invalid())?;
        if address == 0 || address.to_string() != key {
            return Err(invalid());
        }
        let versions = value[3]
            .as_array()
            .filter(|a| a.len() <= 64)
            .ok_or_else(invalid)?
            .iter()
            .map(|v| CausalVersion::decode(&v.to_string()))
            .collect::<Result<_, _>>()?;
        let result = CausalMerkleLeaf {
            root: text(&value[1])?.into(),
            address,
            versions,
            proof: wire_proof(&value[4], 16)?,
        };
        Self::verify_leaf(root, &result)?;
        Ok(result)
    }
}
pub trait CausalMerkleSource {
    fn root(&self) -> &str;
    fn node(&mut self, prefix: &str) -> Result<CausalMerkleNode, LinkError>;
    fn leaf(&mut self, address: u64) -> Result<CausalMerkleLeaf, LinkError>;
}
impl CausalMerkleSource for CausalMerkleSnapshot {
    fn root(&self) -> &str {
        self.root()
    }
    fn node(&mut self, prefix: &str) -> Result<CausalMerkleNode, LinkError> {
        CausalMerkleSnapshot::node(self, prefix)
    }
    fn leaf(&mut self, address: u64) -> Result<CausalMerkleLeaf, LinkError> {
        CausalMerkleSnapshot::leaf(self, address)
    }
}
/// Owned traversal frontier. Reuse only with the same monotone destination journal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CausalPullCursor {
    root: String,
    author: String,
    pending: Vec<String>,
}
impl CausalPullCursor {
    pub fn root(&self) -> &str {
        &self.root
    }
}
#[derive(Clone, Debug)]
pub struct CausalPullOptions {
    pub max_node_requests: usize,
    pub max_leaf_requests: usize,
    pub max_version_bytes: usize,
    pub continuation: Option<CausalPullCursor>,
}
impl Default for CausalPullOptions {
    fn default() -> Self {
        Self {
            max_node_requests: 4096,
            max_leaf_requests: 64,
            max_version_bytes: 8_388_608,
            continuation: None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CausalPullProgress {
    pub root: String,
    pub node_requests: usize,
    pub leaf_requests: usize,
    pub version_bytes: usize,
    pub changed_leaves: usize,
    pub complete: bool,
    pub continuation: Option<CausalPullCursor>,
}
pub fn pull_causal<R: CausalMerkleSource, S: AtomicJournalStore>(
    source: &mut R,
    destination: &mut CausalReplica<S>,
    options: CausalPullOptions,
) -> Result<CausalPullProgress, LinkError> {
    if !(1..=65536).contains(&options.max_node_requests)
        || !(1..=4096).contains(&options.max_leaf_requests)
        || !(1..=8_388_608).contains(&options.max_version_bytes)
    {
        return Err(invalid());
    }
    let root = source.root().to_owned();
    hash(&root)?;
    if options
        .continuation
        .as_ref()
        .is_some_and(|cursor| cursor.root != root)
    {
        return Err(invalid());
    }
    let state = destination.snapshot()?;
    let local = CausalMerkleSnapshot::new(&state.versions, CausalMerkleOptions::default())?;
    let mut pending = match options.continuation {
        Some(cursor) => {
            if cursor.author != state.author {
                return Err(invalid());
            }
            cursor.pending
        }
        None => vec![String::new()],
    };
    let mut progress = CausalPullProgress {
        root: root.clone(),
        node_requests: 0,
        leaf_requests: 0,
        version_bytes: 0,
        changed_leaves: 0,
        complete: false,
        continuation: None,
    };
    if root == local.root || root == hex(&empty()[0]) {
        pending.clear();
    }
    let paused = |mut progress: CausalPullProgress, mut pending: Vec<String>, path: String| {
        pending.push(path);
        progress.continuation = Some(CausalPullCursor {
            root: root.clone(),
            author: state.author.clone(),
            pending,
        });
        Ok(progress)
    };
    while let Some(path) = pending.pop() {
        if path.len() == 16 {
            if progress.leaf_requests == options.max_leaf_requests {
                return paused(progress, pending, path);
            }
            progress.leaf_requests += 1;
            let address = u64::from_str_radix(&path, 16).map_err(|_| invalid())?;
            let response = source.leaf(address)?;
            if response.address != address {
                return Err(invalid());
            }
            let versions = CausalMerkleSnapshot::verify_leaf(&root, &response)?;
            let bytes: usize = versions
                .iter()
                .map(|v| v.encode().map(|s| s.len()))
                .collect::<Result<Vec<_>, _>>()?
                .iter()
                .sum();
            if progress.version_bytes + bytes > options.max_version_bytes {
                return paused(progress, pending, path);
            }
            progress.version_bytes += bytes;
            if destination.receive(&versions)? {
                progress.changed_leaves += 1;
            }
        } else {
            if progress.node_requests == options.max_node_requests {
                return paused(progress, pending, path);
            }
            progress.node_requests += 1;
            let response = source.node(&path)?;
            if response.prefix != path {
                return Err(invalid());
            }
            let remote = CausalMerkleSnapshot::verify_node(&root, &response)?;
            let current = local.children(&path);
            for digit in (0..16).rev() {
                if remote[digit] != hex(&current[digit])
                    && remote[digit] != hex(&empty()[path.len() + 1])
                {
                    pending.push(format!("{path}{digit:x}"));
                }
            }
        }
    }
    progress.complete = true;
    Ok(progress)
}
