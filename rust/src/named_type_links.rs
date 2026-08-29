//! [`NamedTypeLinks`], the storage interface every layer of the CLI is written
//! against, and its implementations for the plain store and for each decorator.
//!
//! A store that implements this trait can be dropped anywhere in the stack, so
//! an application can add its own layer -- a cache, a permission check, a
//! remote store -- without the query processor knowing about it.

use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::path::Path;

use crate::error::LinkError;
use crate::link::Link;
use crate::link_storage::{ChangeObserver, LinkStorage};
use crate::named_types::{NamedTypes, NamedTypesDecorator};

pub trait NamedTypeLinks {
    fn create(&mut self, source: u32, target: u32) -> u32;
    fn ensure_created(&mut self, id: u32) -> u32;
    fn try_ensure_created(&mut self, id: u32) -> Result<u32> {
        if id == 0 || id == u32::MAX {
            return Err(LinkError::InvalidFormat(format!(
                "Cannot ensure unsupported link address {id}"
            ))
            .into());
        }

        Ok(self.ensure_created(id))
    }
    fn get_link(&mut self, id: u32) -> Option<Link>;
    fn exists(&mut self, id: u32) -> bool;
    fn update(&mut self, id: u32, source: u32, target: u32) -> Result<Link>;
    fn delete(&mut self, id: u32) -> Result<Link>;
    /// [`Self::delete`], reporting every change the deletion caused.
    ///
    /// Deleting a link cascades into every link that still used it, and each of
    /// those removals is a change in its own right. The C# CLI sees them
    /// because `AdvancedMixedQueryProcessor.RemoveLinks` hands a handler to the
    /// store:
    ///
    /// ```csharp
    /// links.Delete(link, (before, after) =>
    ///     options.ChangesHandler?.Invoke(before, after) ?? links.Constants.Continue);
    /// ```
    ///
    /// The default implementation reports only the link that was asked for,
    /// which is correct for stores that cannot cascade; every decorator over a
    /// cascading store overrides it.
    fn delete_observed(&mut self, id: u32, observer: ChangeObserver<'_>) -> Result<Link> {
        let before = self.delete(id)?;
        observer(before, Link::null());
        Ok(before)
    }
    fn all_links(&mut self) -> Vec<Link>;
    fn search(&mut self, source: u32, target: u32) -> Option<u32>;
    fn get_or_create(&mut self, source: u32, target: u32) -> u32;
    fn get_name(&mut self, id: u32) -> Result<Option<String>>;
    fn set_name(&mut self, id: u32, name: &str) -> Result<u32>;
    fn get_by_name(&mut self, name: &str) -> Result<Option<u32>>;
    fn remove_name(&mut self, id: u32) -> Result<()>;
    fn save(&mut self) -> Result<()>;

    fn get_or_create_named(&mut self, name: &str) -> Result<u32> {
        if let Some(id) = self.get_by_name(name)? {
            return Ok(id);
        }

        let id = self.create(0, 0);
        self.set_name(id, name)?;
        self.update(id, id, id)?;
        Ok(id)
    }

    fn format_reference(&mut self, id: u32) -> Result<String> {
        Ok(self
            .get_name(id)?
            .map(|name| escape_lino_reference(&name))
            .unwrap_or_else(|| id.to_string()))
    }

    fn format_lino(&mut self, link: &Link) -> Result<String> {
        Ok(format!(
            "({}: {} {})",
            self.format_reference(link.index)?,
            self.format_reference(link.source)?,
            self.format_reference(link.target)?
        ))
    }

    fn lino_lines(&mut self) -> Result<Vec<String>> {
        let mut links = self.all_links();
        links.sort_by_key(|link| link.index);

        links
            .iter()
            .map(|link| self.format_lino(link))
            .collect::<Result<Vec<_>>>()
    }

    fn write_lino_output<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        let path = path.as_ref();
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .with_context(|| format!("Failed to create LiNo output: {}", path.display()))?;

        let mut writer = BufWriter::new(file);
        for line in self.lino_lines()? {
            writeln!(writer, "{line}")?;
        }
        writer.flush()?;
        Ok(())
    }

    fn print_all_lino(&mut self) -> Result<()> {
        for line in self.lino_lines()? {
            println!("{line}");
        }
        Ok(())
    }

    fn print_change_lino(&mut self, before: &Option<Link>, after: &Option<Link>) -> Result<()> {
        let before_text = before
            .map(|link| self.format_lino(&link))
            .transpose()?
            .unwrap_or_default();
        let after_text = after
            .map(|link| self.format_lino(&link))
            .transpose()?
            .unwrap_or_default();
        println!("({before_text}) ({after_text})");
        Ok(())
    }

    fn format_structure(&mut self, id: u32) -> Result<String> {
        let mut visited = HashSet::new();
        self.format_structure_recursive(id, &mut visited)
    }

    fn format_structure_recursive(
        &mut self,
        id: u32,
        visited: &mut HashSet<u32>,
    ) -> Result<String> {
        let link = self.get_link(id).ok_or(LinkError::not_found(id))?;
        if !visited.insert(id) {
            return self.format_reference(id);
        }

        let source = if self.exists(link.source) && !visited.contains(&link.source) {
            self.format_structure_recursive(link.source, visited)?
        } else {
            self.format_reference(link.source)?
        };
        let target = self.format_reference(link.target)?;
        let index = self.format_reference(link.index)?;
        visited.remove(&id);

        Ok(format!("({index}: {source} {target})"))
    }
}

impl NamedTypeLinks for LinkStorage {
    fn create(&mut self, source: u32, target: u32) -> u32 {
        LinkStorage::create(self, source, target)
    }

    fn ensure_created(&mut self, id: u32) -> u32 {
        LinkStorage::ensure_created(self, id)
    }

    fn get_link(&mut self, id: u32) -> Option<Link> {
        self.get(id).copied()
    }

    fn exists(&mut self, id: u32) -> bool {
        LinkStorage::exists(self, id)
    }

    fn update(&mut self, id: u32, source: u32, target: u32) -> Result<Link> {
        LinkStorage::update(self, id, source, target)
    }

    fn delete(&mut self, id: u32) -> Result<Link> {
        LinkStorage::delete(self, id)
    }

    fn delete_observed(&mut self, id: u32, observer: ChangeObserver<'_>) -> Result<Link> {
        LinkStorage::delete_observed(self, id, observer)
    }

    fn all_links(&mut self) -> Vec<Link> {
        self.all().into_iter().copied().collect()
    }

    fn search(&mut self, source: u32, target: u32) -> Option<u32> {
        LinkStorage::search(self, source, target)
    }

    fn get_or_create(&mut self, source: u32, target: u32) -> u32 {
        LinkStorage::get_or_create(self, source, target)
    }

    fn get_name(&mut self, id: u32) -> Result<Option<String>> {
        Ok(LinkStorage::get_name(self, id).cloned())
    }

    fn set_name(&mut self, id: u32, name: &str) -> Result<u32> {
        LinkStorage::set_name(self, id, name);
        Ok(id)
    }

    fn get_by_name(&mut self, name: &str) -> Result<Option<u32>> {
        Ok(LinkStorage::get_by_name(self, name))
    }

    fn remove_name(&mut self, id: u32) -> Result<()> {
        LinkStorage::remove_name(self, id);
        Ok(())
    }

    fn save(&mut self) -> Result<()> {
        LinkStorage::save(self)
    }

    fn get_or_create_named(&mut self, name: &str) -> Result<u32> {
        Ok(LinkStorage::get_or_create_named(self, name))
    }
}

impl NamedTypeLinks for NamedTypesDecorator {
    fn create(&mut self, source: u32, target: u32) -> u32 {
        NamedTypesDecorator::create(self, source, target)
    }

    fn ensure_created(&mut self, id: u32) -> u32 {
        NamedTypesDecorator::ensure_created(self, id)
    }

    fn get_link(&mut self, id: u32) -> Option<Link> {
        self.get(id).copied()
    }

    fn exists(&mut self, id: u32) -> bool {
        NamedTypesDecorator::exists(self, id)
    }

    fn update(&mut self, id: u32, source: u32, target: u32) -> Result<Link> {
        NamedTypesDecorator::update(self, id, source, target)
    }

    fn delete(&mut self, id: u32) -> Result<Link> {
        NamedTypesDecorator::delete(self, id)
    }

    fn delete_observed(&mut self, id: u32, observer: ChangeObserver<'_>) -> Result<Link> {
        NamedTypesDecorator::delete_observed(self, id, observer)
    }

    fn all_links(&mut self) -> Vec<Link> {
        self.all().into_iter().copied().collect()
    }

    fn search(&mut self, source: u32, target: u32) -> Option<u32> {
        NamedTypesDecorator::search(self, source, target)
    }

    fn get_or_create(&mut self, source: u32, target: u32) -> u32 {
        NamedTypesDecorator::get_or_create(self, source, target)
    }

    fn get_name(&mut self, id: u32) -> Result<Option<String>> {
        NamedTypes::get_name(self, id)
    }

    fn set_name(&mut self, id: u32, name: &str) -> Result<u32> {
        NamedTypes::set_name(self, id, name)
    }

    fn get_by_name(&mut self, name: &str) -> Result<Option<u32>> {
        NamedTypes::get_by_name(self, name)
    }

    fn remove_name(&mut self, id: u32) -> Result<()> {
        NamedTypes::remove_name(self, id)
    }

    fn save(&mut self) -> Result<()> {
        NamedTypesDecorator::save(self)
    }
}

impl NamedTypeLinks for crate::transactions::TransactionsDecorator {
    fn create(&mut self, source: u32, target: u32) -> u32 {
        crate::transactions::TransactionsDecorator::create(self, source, target)
            .expect("TransactionsDecorator::create failed in NamedTypeLinks bridge")
    }

    fn ensure_created(&mut self, id: u32) -> u32 {
        crate::transactions::TransactionsDecorator::ensure_created(self, id)
            .expect("TransactionsDecorator::ensure_created failed in NamedTypeLinks bridge")
    }

    fn get_link(&mut self, id: u32) -> Option<Link> {
        self.get(id).copied()
    }

    fn exists(&mut self, id: u32) -> bool {
        crate::transactions::TransactionsDecorator::exists(self, id)
    }

    fn update(&mut self, id: u32, source: u32, target: u32) -> Result<Link> {
        Ok(crate::transactions::TransactionsDecorator::update(
            self, id, source, target,
        )?)
    }

    fn delete(&mut self, id: u32) -> Result<Link> {
        Ok(crate::transactions::TransactionsDecorator::delete(
            self, id,
        )?)
    }

    fn delete_observed(&mut self, id: u32, observer: ChangeObserver<'_>) -> Result<Link> {
        Ok(crate::transactions::TransactionsDecorator::delete_observed(
            self, id, observer,
        )?)
    }

    fn all_links(&mut self) -> Vec<Link> {
        self.all().into_iter().copied().collect()
    }

    fn search(&mut self, source: u32, target: u32) -> Option<u32> {
        crate::transactions::TransactionsDecorator::search(self, source, target)
    }

    fn get_or_create(&mut self, source: u32, target: u32) -> u32 {
        crate::transactions::TransactionsDecorator::get_or_create(self, source, target)
            .expect("TransactionsDecorator::get_or_create failed in NamedTypeLinks bridge")
    }

    fn get_name(&mut self, id: u32) -> Result<Option<String>> {
        NamedTypes::get_name(self.inner_mut(), id)
    }

    fn set_name(&mut self, id: u32, name: &str) -> Result<u32> {
        NamedTypes::set_name(self.inner_mut(), id, name)
    }

    fn get_by_name(&mut self, name: &str) -> Result<Option<u32>> {
        NamedTypes::get_by_name(self.inner_mut(), name)
    }

    fn remove_name(&mut self, id: u32) -> Result<()> {
        NamedTypes::remove_name(self.inner_mut(), id)
    }

    fn save(&mut self) -> Result<()> {
        Ok(crate::transactions::TransactionsDecorator::save(self)?)
    }
}

impl NamedTypeLinks for crate::version_control::VersionControlDecorator {
    fn create(&mut self, source: u32, target: u32) -> u32 {
        crate::version_control::VersionControlDecorator::create(self, source, target)
            .expect("VersionControlDecorator::create failed in NamedTypeLinks bridge")
    }

    fn ensure_created(&mut self, id: u32) -> u32 {
        crate::version_control::VersionControlDecorator::ensure_created(self, id)
    }

    fn get_link(&mut self, id: u32) -> Option<Link> {
        self.get(id).copied()
    }

    fn exists(&mut self, id: u32) -> bool {
        crate::version_control::VersionControlDecorator::exists(self, id)
    }

    fn update(&mut self, id: u32, source: u32, target: u32) -> Result<Link> {
        crate::version_control::VersionControlDecorator::update(self, id, source, target)
    }

    fn delete(&mut self, id: u32) -> Result<Link> {
        crate::version_control::VersionControlDecorator::delete(self, id)
    }

    fn delete_observed(&mut self, id: u32, observer: ChangeObserver<'_>) -> Result<Link> {
        crate::version_control::VersionControlDecorator::delete_observed(self, id, observer)
    }

    fn all_links(&mut self) -> Vec<Link> {
        self.all().into_iter().copied().collect()
    }

    fn search(&mut self, source: u32, target: u32) -> Option<u32> {
        crate::version_control::VersionControlDecorator::search(self, source, target)
    }

    fn get_or_create(&mut self, source: u32, target: u32) -> u32 {
        crate::version_control::VersionControlDecorator::get_or_create(self, source, target)
            .expect("VersionControlDecorator::get_or_create failed in NamedTypeLinks bridge")
    }

    fn get_name(&mut self, id: u32) -> Result<Option<String>> {
        NamedTypes::get_name(self.transactions_mut().inner_mut(), id)
    }

    fn set_name(&mut self, id: u32, name: &str) -> Result<u32> {
        NamedTypes::set_name(self.transactions_mut().inner_mut(), id, name)
    }

    fn get_by_name(&mut self, name: &str) -> Result<Option<u32>> {
        NamedTypes::get_by_name(self.transactions_mut().inner_mut(), name)
    }

    fn remove_name(&mut self, id: u32) -> Result<()> {
        NamedTypes::remove_name(self.transactions_mut().inner_mut(), id)
    }

    fn save(&mut self) -> Result<()> {
        crate::version_control::VersionControlDecorator::save(self)
    }
}

pub fn escape_lino_reference(reference: &str) -> String {
    if reference.is_empty() || reference.trim().is_empty() {
        return String::new();
    }

    let has_single_quote = reference.contains('\'');
    let has_double_quote = reference.contains('"');
    let needs_quoting = reference.contains(':')
        || reference.contains('(')
        || reference.contains(')')
        || reference.contains(' ')
        || reference.contains('\t')
        || reference.contains('\n')
        || reference.contains('\r')
        || has_single_quote
        || has_double_quote;

    if has_single_quote && has_double_quote {
        return format!("'{}'", reference.replace('\'', "\\'"));
    }

    if has_double_quote {
        return format!("'{reference}'");
    }

    if has_single_quote {
        return format!("\"{reference}\"");
    }

    if needs_quoting {
        return format!("'{reference}'");
    }

    reference.to_string()
}
