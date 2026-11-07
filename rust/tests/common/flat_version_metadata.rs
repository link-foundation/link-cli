//! Metadata-only fixture: isolate ancestry from Unicode name construction costs.
use anyhow::Result;
use link_cli::{
    CommitMode, GenericVersionControlDecorator, Link, LinkStorage, LogRetentionPolicy,
    NamedTypeLinks, NamedTypesDecorator, TransactionsDecorator,
};
use std::collections::HashMap;

pub struct FlatMetadata {
    inner: NamedTypesDecorator,
    names: HashMap<u32, String>,
}
impl FlatMetadata {
    pub fn new() -> Self {
        Self {
            inner: NamedTypesDecorator::from_link_storages(
                LinkStorage::in_memory(false),
                LinkStorage::in_memory(false),
            ),
            names: HashMap::new(),
        }
    }
    pub fn deep_version(depth: usize) -> Result<GenericVersionControlDecorator<Self>> {
        let named = || {
            NamedTypesDecorator::from_link_storages(
                LinkStorage::in_memory(false),
                LinkStorage::in_memory(false),
            )
        };
        let tx = TransactionsDecorator::new(
            named(),
            named(),
            LogRetentionPolicy::Infinite,
            CommitMode::Sync,
            false,
        )?;
        let mut branches = Self::new();
        let mut marker = |text: String| -> Result<()> {
            let id = branches.try_create(0, 0)?;
            branches.set_name(id, &text)?;
            Ok(())
        };
        marker("__vc:branch:main:parent=:fork=0:head=0".into())?;
        for index in 1..=depth {
            let parent = if index == 1 {
                "main".into()
            } else {
                format!("branch-{}", index - 1)
            };
            marker(format!(
                "__vc:branch:branch-{index}:parent={parent}:fork=0:head=0"
            ))?;
        }
        marker(format!("__vc:current=branch-{depth}"))?;
        marker("__vc:applied=0".into())?;
        GenericVersionControlDecorator::new(tx, branches, false)
    }
}
impl NamedTypeLinks for FlatMetadata {
    fn create(&mut self, source: u32, target: u32) -> u32 {
        self.inner.create(source, target)
    }
    fn ensure_created(&mut self, id: u32) -> u32 {
        self.inner.ensure_created(id)
    }
    fn get_link(&mut self, id: u32) -> Option<Link> {
        self.inner.get(id).copied()
    }
    fn exists(&mut self, id: u32) -> bool {
        self.inner.exists(id)
    }
    fn update_observed(
        &mut self,
        id: u32,
        source: u32,
        target: u32,
        observer: link_cli::link_storage::ChangeObserver<'_>,
    ) -> Result<Link> {
        self.inner.update_observed(id, source, target, observer)
    }
    fn delete_observed(
        &mut self,
        id: u32,
        observer: link_cli::link_storage::ChangeObserver<'_>,
    ) -> Result<Link> {
        self.names.remove(&id);
        self.inner.delete_observed(id, observer)
    }
    fn all_links(&mut self) -> Vec<Link> {
        self.inner.all().into_iter().copied().collect()
    }
    fn search(&mut self, source: u32, target: u32) -> Option<u32> {
        self.inner.search(source, target)
    }
    fn get_or_create(&mut self, source: u32, target: u32) -> u32 {
        self.inner.get_or_create(source, target)
    }
    fn get_name(&mut self, id: u32) -> Result<Option<String>> {
        Ok(self.names.get(&id).cloned())
    }
    fn set_name(&mut self, id: u32, name: &str) -> Result<u32> {
        self.names.retain(|_, previous| previous != name);
        self.names.insert(id, name.to_owned());
        Ok(id)
    }
    fn get_by_name(&mut self, name: &str) -> Result<Option<u32>> {
        Ok(self
            .names
            .iter()
            .find_map(|(id, value)| (value == name).then_some(*id)))
    }
    fn remove_name(&mut self, id: u32) -> Result<()> {
        self.names.remove(&id);
        Ok(())
    }
    fn save(&mut self) -> Result<()> {
        Ok(())
    }
}
