//! Independent shallow recursive oracle plus finite deep public-API contracts.
use anyhow::{bail, Result};
use link_cli::{Link, LinkStorage, NamedTypeLinks};
use std::collections::HashSet;

fn oracle<S: NamedTypeLinks>(store: &mut S, id: u32, path: &mut HashSet<u32>) -> Result<String> {
    let link = store
        .get_link(id)
        .ok_or_else(|| anyhow::anyhow!("missing {id}"))?;
    if !path.insert(id) {
        return store.format_reference(id);
    }
    let left = if store.exists(link.source) && !path.contains(&link.source) {
        oracle(store, link.source, path)?
    } else {
        store.format_reference(link.source)?
    };
    let right = store.format_reference(link.target)?;
    let name = store.format_reference(link.index)?;
    path.remove(&id);
    Ok(format!("({name}: {left} {right})"))
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn finite_graphs_match_independent_source_spine_oracle() -> Result<()> {
    for seed in 0..24 {
        let mut links = LinkStorage::in_memory(false);
        for index in 0..8 {
            links.create((index * 3 + seed) % 10, (index + seed * 3) % 10);
        }
        for (id, name) in [
            "",
            "snow 雪",
            "both'\"quotes",
            "line\nzero\0",
            "42",
            "plain",
            "a:b",
            "~1{31}",
        ]
        .into_iter()
        .enumerate()
        {
            links.set_name(id as u32 + 1, name);
        }
        let before = links.all().into_iter().copied().collect::<Vec<_>>();
        for root in 1..=8 {
            let expected = oracle(&mut links, root, &mut HashSet::new())?;
            assert_eq!(
                links.format_structure(root)?,
                expected,
                "concrete seed {seed}, root {root}"
            );
            assert_eq!(
                NamedTypeLinks::format_structure(&mut links, root)?,
                expected,
                "trait seed {seed}, root {root}"
            );
        }
        assert_eq!(links.all().into_iter().copied().collect::<Vec<_>>(), before);
    }
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn caller_supplied_paths_and_missing_roots_keep_their_contract() -> Result<()> {
    let mut links = LinkStorage::in_memory(false);
    let leaf = links.create(0, 0);
    let root = links.create(leaf, leaf);
    links.set_name(leaf, "leaf 雪");
    links.set_name(root, "root");
    for initial in [
        HashSet::new(),
        HashSet::from([leaf]),
        HashSet::from([root]),
        HashSet::from([999]),
    ] {
        let mut path = initial.clone();
        let expected = oracle(&mut links, root, &mut initial.clone())?;
        assert_eq!(links.format_structure_recursive(root, &mut path)?, expected);
        assert_eq!(path, initial);
    }
    let mut path = HashSet::from([999]);
    assert!(links.format_structure_recursive(999, &mut path).is_err());
    assert_eq!(path, HashSet::from([999]));
    assert!(links.format_structure(999).is_err());
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn finite_deep_output_is_exact_for_both_rust_entry_points() -> Result<()> {
    let mut links = LinkStorage::in_memory(false);
    let mut ids = Vec::new();
    let mut root = 0;
    for _ in 0..2048 {
        root = links.create(root, 0);
        ids.push(root);
    }
    let mut expected = ids
        .iter()
        .rev()
        .map(|id| format!("({id}: "))
        .collect::<String>();
    expected.push('0');
    expected.push_str(&" 0)".repeat(ids.len()));
    assert_eq!(links.format_structure(root)?, expected);
    assert_eq!(
        NamedTypeLinks::format_structure(&mut links, root)?,
        expected
    );
    Ok(())
}

struct TraceStore {
    links: LinkStorage,
    events: Vec<String>,
    fail_name: Option<usize>,
    names_read: usize,
}

impl Default for TraceStore {
    fn default() -> Self {
        Self {
            links: LinkStorage::in_memory(false),
            events: Vec::new(),
            fail_name: None,
            names_read: 0,
        }
    }
}

impl NamedTypeLinks for TraceStore {
    fn create(&mut self, source: u32, target: u32) -> u32 {
        self.links.create(source, target)
    }
    fn get_link(&mut self, id: u32) -> Option<Link> {
        self.events.push(format!("get {id}"));
        self.links.get(id).copied()
    }
    fn exists(&mut self, id: u32) -> bool {
        self.events.push(format!("exists {id}"));
        self.links.exists(id)
    }
    fn get_name(&mut self, id: u32) -> Result<Option<String>> {
        self.events.push(format!("name {id}"));
        let index = self.names_read;
        self.names_read += 1;
        if self.fail_name == Some(index) {
            bail!("name fault {index}");
        }
        Ok(self.links.get_name(id).cloned())
    }
    fn ensure_created(&mut self, _: u32) -> u32 {
        unreachable!()
    }
    fn update_observed(
        &mut self,
        _: u32,
        _: u32,
        _: u32,
        _: link_cli::link_storage::ChangeObserver<'_>,
    ) -> Result<Link> {
        unreachable!()
    }
    fn delete_observed(
        &mut self,
        _: u32,
        _: link_cli::link_storage::ChangeObserver<'_>,
    ) -> Result<Link> {
        unreachable!()
    }
    fn all_links(&mut self) -> Vec<Link> {
        unreachable!()
    }
    fn search(&mut self, _: u32, _: u32) -> Option<u32> {
        unreachable!()
    }
    fn get_or_create(&mut self, _: u32, _: u32) -> u32 {
        unreachable!()
    }
    fn set_name(&mut self, _: u32, _: &str) -> Result<u32> {
        unreachable!()
    }
    fn get_by_name(&mut self, _: &str) -> Result<Option<u32>> {
        unreachable!()
    }
    fn remove_name(&mut self, _: u32) -> Result<()> {
        unreachable!()
    }
    fn save(&mut self) -> Result<()> {
        unreachable!()
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn reference_failures_preserve_read_order_and_path_progress() -> Result<()> {
    for fail_name in [
        None,
        Some(0),
        Some(1),
        Some(2),
        Some(3),
        Some(4),
        Some(5),
        Some(6),
    ] {
        let make = || {
            let mut store = TraceStore {
                fail_name,
                ..Default::default()
            };
            store.create(0, 70);
            store.create(1, 71);
            store.create(2, 72);
            store
        };
        let mut expected_store = make();
        let mut actual_store = make();
        let mut expected_path = HashSet::from([999]);
        let mut actual_path = expected_path.clone();
        let expected =
            oracle(&mut expected_store, 3, &mut expected_path).map_err(|error| error.to_string());
        let actual = actual_store
            .format_structure_recursive(3, &mut actual_path)
            .map_err(|error| error.to_string());
        assert_eq!(actual, expected);
        assert_eq!(actual_store.events, expected_store.events);
        assert_eq!(actual_path, expected_path);
        actual_store.fail_name = None;
        assert!(actual_store.format_structure(3).is_ok());
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn persistent_output_matches_after_repeated_reopening() -> Result<()> {
    use link_cli::NamedTypesDecorator;
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("data.links");
    let mut links = NamedTypesDecorator::new(&path, false)?;
    let first = links.create(0, 0);
    links.set_name(first, "leaf 雪")?;
    let root = links.create(first, first);
    links.set_name(root, "both'\"quotes")?;
    let expected = links.format_structure(root)?;
    links.save()?;
    drop(links);
    for _ in 0..2 {
        let mut links = NamedTypesDecorator::new(&path, false)?;
        assert_eq!(links.format_structure(root)?, expected);
    }
    Ok(())
}
