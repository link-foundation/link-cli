//! Finite offline proof exchange between independently authored native stores.
use link_cli::engine::{
    causal::{CausalPayload, CausalVersion},
    causal_merkle::{
        pull_causal, CausalMerkleLeaf, CausalMerkleNode, CausalMerkleOptions, CausalMerkleSnapshot,
        CausalMerkleSource, CausalPullOptions,
    },
    causal_replica::{CausalReplica, CausalReplicaOptions},
    cluster::{AtomicJournalStore, JournalVersion},
    lsm::{FileLsmJournalStore, FileLsmOptions, LsmOptions, LsmStoreOptions},
};
use link_cli::LinkError;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
};
const A: &str = "00000000000000000000000000000001";
const B: &str = "00000000000000000000000000000002";
const LOCAL: &str = "0000000000000000000000000000000d";
fn row(name: &str) -> Option<CausalPayload> {
    Some(CausalPayload {
        source: 2,
        target: 3,
        name: Some(name.into()),
    })
}
fn token(n: u64) -> String {
    format!("{n:032x}")
}
fn wires(values: &[CausalVersion]) -> Vec<String> {
    let mut result: Vec<_> = values.iter().map(|v| v.encode().unwrap()).collect();
    result.sort();
    result
}
struct CutStore {
    physical: FileLsmJournalStore,
    action: String,
}
impl AtomicJournalStore for CutStore {
    fn supports_durable_flush(&self) -> bool {
        self.physical.supports_durable_flush()
    }
    fn read(&mut self, keys: &[String]) -> Result<Vec<JournalVersion>, LinkError> {
        self.physical.read(keys)
    }
    fn compare_exchange(
        &mut self,
        keys: &[String],
        expected: &[JournalVersion],
        updates: &[Option<Vec<String>>],
    ) -> Result<bool, LinkError> {
        if self.action == "cut-before" {
            std::process::exit(73);
        }
        let result = self.physical.compare_exchange(keys, expected, updates)?;
        if self.action == "cut-after" && result {
            std::process::exit(73);
        }
        Ok(result)
    }
}
struct Source {
    root: String,
    nodes: BTreeMap<String, String>,
    leaves: BTreeMap<u64, String>,
}
impl CausalMerkleSource for Source {
    fn root(&self) -> &str {
        &self.root
    }
    fn node(&mut self, path: &str) -> Result<CausalMerkleNode, LinkError> {
        CausalMerkleSnapshot::decode_node(
            &self.root,
            self.nodes
                .get(path)
                .ok_or_else(|| LinkError::InvalidFormat("missing node".into()))?,
        )
    }
    fn leaf(&mut self, key: u64) -> Result<CausalMerkleLeaf, LinkError> {
        CausalMerkleSnapshot::decode_leaf(
            &self.root,
            self.leaves
                .get(&key)
                .ok_or_else(|| LinkError::InvalidFormat("missing leaf".into()))?,
        )
    }
}
fn check(
    owner: &mut CausalReplica<CutStore>,
    following: bool,
    values: &[CausalVersion],
) -> Result<(), LinkError> {
    let selected = &values[0];
    let other = &values[1];
    let concurrent = &values[2];
    let resolved = &values[3];
    let state = owner.snapshot()?;
    assert_eq!(state.last_counter, if following { 2 } else { 1 });
    let expected = if following {
        vec![resolved.clone(), other.clone()]
    } else {
        vec![selected.clone(), concurrent.clone(), other.clone()]
    };
    assert_eq!(wires(&state.versions), wires(&expected));
    assert_eq!(
        owner.write(&token(1), 1, &[], row("concurrent"))?,
        *concurrent
    );
    Ok(())
}
pub fn run(
    action: &str,
    directory: &str,
    mirrors: &str,
    message_file: &str,
) -> Result<(), LinkError> {
    assert!(matches!(
        action,
        "export"
            | "seed"
            | "pull"
            | "read-pulled"
            | "following"
            | "read-following"
            | "cut-before"
            | "cut-after"
    ));
    assert!(matches!(mirrors, "plain" | "mirrored"));
    let mirrored = mirrors == "mirrored";
    let options = FileLsmOptions {
        store: LsmStoreOptions {
            limits: LsmOptions {
                max_keys: 32,
                max_bytes: 65536,
                buffer_keys: 16,
                buffer_bytes: 32768,
                max_runs: 8,
                ..Default::default()
            },
            ..Default::default()
        },
        max_pages: 128,
        mirrored_root: mirrored,
        mirrored_banks: mirrored,
        mirrored_tables: mirrored,
    };
    let physical = FileLsmJournalStore::open(directory, options)?;
    let mut owner = CausalReplica::new(
        CutStore {
            physical,
            action: action.into(),
        },
        "replica",
        if action == "export" { A } else { LOCAL },
        CausalReplicaOptions {
            require_durable_flush: true,
            ..Default::default()
        },
    )?;
    let initial = CausalVersion::advance(1, A, 0, &[], row("initial\u{feff}雪😀\0"))?;
    let deletion = CausalVersion::advance(1, B, 0, &[initial], None)?;
    let selected = CausalVersion::advance(1, A, 1, &[deletion], row("selected\u{feff}雪😀\0"))?;
    let other = CausalVersion::advance(2, A, 2, &[], None)?;
    let concurrent = CausalVersion::advance(1, LOCAL, 0, &[], row("concurrent"))?;
    let resolved =
        CausalVersion::advance(1, LOCAL, 1, &[selected.clone(), concurrent.clone()], None)?;
    let values = vec![
        selected.clone(),
        other.clone(),
        concurrent.clone(),
        resolved.clone(),
    ];
    match action {
        "export" => {
            let snapshot = CausalMerkleSnapshot::new(
                &owner.snapshot()?.versions,
                CausalMerkleOptions::default(),
            )?;
            let mut lines = vec!["causal-bundle1".into(), snapshot.root().into()];
            let mut prefixes = BTreeSet::new();
            for value in [&selected, &other] {
                let path = format!("{:016x}", value.address);
                for depth in 0..16 {
                    prefixes.insert(path[..depth].to_owned());
                }
            }
            for prefix in prefixes {
                lines.push(CausalMerkleSnapshot::encode_node(&snapshot.node(&prefix)?)?);
            }
            for value in [&selected, &other] {
                lines.push(CausalMerkleSnapshot::encode_leaf(
                    &snapshot.leaf(value.address)?,
                )?);
            }
            let wire = lines.join("\n") + "\n";
            assert!(wire.len() <= 524288);
            std::fs::write(message_file, wire)?;
        }
        "seed" => {
            assert_eq!(
                owner.write(&token(1), 1, &[], row("concurrent"))?,
                concurrent
            );
        }
        "read-pulled" | "read-following" => check(&mut owner, action == "read-following", &values)?,
        "following" => {
            check(&mut owner, false, &values)?;
            let observed = owner.read(1)?;
            assert_eq!(owner.write(&token(2), 1, &observed, None)?, resolved);
            check(&mut owner, true, &values)?;
        }
        _ => {
            let mut bytes = Vec::new();
            std::fs::File::open(message_file)?
                .take(524289)
                .read_to_end(&mut bytes)?;
            assert!(bytes.len() <= 524288);
            let wire = String::from_utf8(bytes)
                .map_err(|_| LinkError::InvalidFormat("invalid bundle UTF8".into()))?;
            let lines: Vec<_> = wire.split('\n').collect();
            assert!(
                (4..=43).contains(&lines.len())
                    && lines[0] == "causal-bundle1"
                    && lines.last() == Some(&"")
            );
            let mut source = Source {
                root: lines[1].into(),
                nodes: BTreeMap::new(),
                leaves: BTreeMap::new(),
            };
            for wire in &lines[2..lines.len() - 1] {
                if wire.starts_with("[\"causal-node1\",") {
                    let value = CausalMerkleSnapshot::decode_node(&source.root, wire)?;
                    assert!(source.nodes.insert(value.prefix, (*wire).into()).is_none());
                } else {
                    let value = CausalMerkleSnapshot::decode_leaf(&source.root, wire)?;
                    assert!(source
                        .leaves
                        .insert(value.address, (*wire).into())
                        .is_none());
                }
            }
            let mut continuation = None;
            let mut complete = false;
            for _ in 0..40 {
                let progress = pull_causal(
                    &mut source,
                    &mut owner,
                    CausalPullOptions {
                        max_node_requests: 4,
                        max_leaf_requests: 1,
                        continuation,
                        ..Default::default()
                    },
                )?;
                assert!(progress.node_requests <= 4 && progress.leaf_requests <= 1);
                if progress.complete {
                    complete = true;
                    break;
                }
                continuation = progress.continuation;
            }
            assert!(complete);
            check(&mut owner, false, &values)?;
            let retry = pull_causal(&mut source, &mut owner, CausalPullOptions::default())?;
            assert!(retry.complete);
            assert_eq!(retry.changed_leaves, 0);
        }
    }
    owner.into_store().physical.close()?;
    println!("PASS Rust Merkle native LSM {action} {mirrors}");
    Ok(())
}
