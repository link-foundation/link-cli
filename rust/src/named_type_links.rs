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
use crate::link::{GenericLink, Link};
use crate::link_storage::{ChangeObserver, LinkStorage};
use crate::named_types::{NamedTypes, NamedTypesDecorator};
use crate::query_address::QueryAddress;

fn validate_ensure_address<T: QueryAddress>(id: T) -> Result<()> {
    if id == T::from_byte(0) || id == T::MAX {
        return Err(LinkError::InvalidFormat(format!(
            "Cannot ensure unsupported link address {id}"
        ))
        .into());
    }
    Ok(())
}

pub trait NamedTypeLinks<T: QueryAddress = u32> {
    /// A checked transport may restrict numeric input and query expansion.
    fn query_limits(&self) -> Option<crate::query_options::QueryLimits> {
        None
    }
    /// Reserve the actual allocation sequence without staging observable rows.
    /// Backends with a recycling allocator may retain the legacy probe path.
    fn reserve_query_allocations(
        &mut self,
        _processor: &crate::query_processor::GenericQueryProcessor<T>,
        _restriction: &crate::lino_link::LinoLink,
        _substitution: &crate::lino_link::LinoLink,
    ) -> Result<Option<HashSet<T>>> {
        Ok(None)
    }
    fn create(&mut self, source: T, target: T) -> T;
    /// Fallible allocation for backends that can report transaction/address errors.
    fn try_create(&mut self, source: T, target: T) -> Result<T> {
        Ok(self.create(source, target))
    }
    fn ensure_created(&mut self, id: T) -> T;
    fn try_ensure_created(&mut self, id: T) -> Result<T> {
        validate_ensure_address(id)?;
        Ok(self.ensure_created(id))
    }
    fn get_link(&mut self, id: T) -> Option<GenericLink<T>>;
    fn exists(&mut self, id: T) -> bool;
    fn update(&mut self, id: T, source: T, target: T) -> Result<GenericLink<T>> {
        self.update_observed(id, source, target, &mut |_, _| {})
    }
    /// [`Self::update`], reporting every change the update caused.
    ///
    /// An update that would duplicate an existing doublet is resolved into a
    /// merge: the updated link is deleted and its usages are moved to the
    /// duplicate, so one call can make several changes. The C# CLI sees them
    /// because `AdvancedMixedQueryProcessor` hands a handler to the store:
    ///
    /// ```csharp
    /// links.Update(before, after, (beforeState, afterState) =>
    ///     options.ChangesHandler?.Invoke(beforeState, afterState) ?? links.Constants.Continue);
    /// ```
    fn update_observed(
        &mut self,
        id: T,
        source: T,
        target: T,
        observer: ChangeObserver<'_, T>,
    ) -> Result<GenericLink<T>>;
    fn delete(&mut self, id: T) -> Result<GenericLink<T>> {
        self.delete_observed(id, &mut |_, _| {})
    }
    /// [`Self::delete`], reporting every change the deletion caused.
    ///
    /// Deleting a link cascades into every link that still used it, and each of
    /// those removals is a change in its own right, reported the way
    /// `AdvancedMixedQueryProcessor.RemoveLinks` reports them in C#:
    ///
    /// ```csharp
    /// links.Delete(link, (before, after) =>
    ///     options.ChangesHandler?.Invoke(before, after) ?? links.Constants.Continue);
    /// ```
    fn delete_observed(&mut self, id: T, observer: ChangeObserver<'_, T>)
        -> Result<GenericLink<T>>;
    fn all_links(&mut self) -> Vec<GenericLink<T>>;
    fn search(&mut self, source: T, target: T) -> Option<T>;
    fn get_or_create(&mut self, source: T, target: T) -> T;
    /// Fallible pair lookup/allocation for composable query and decorator writes.
    fn try_get_or_create(&mut self, source: T, target: T) -> Result<T> {
        if let Some(id) = self.search(source, target) {
            return Ok(id);
        }
        self.try_create(source, target)
    }
    fn get_name(&mut self, id: T) -> Result<Option<String>>;
    fn set_name(&mut self, id: T, name: &str) -> Result<T>;
    fn get_by_name(&mut self, name: &str) -> Result<Option<T>>;
    fn remove_name(&mut self, id: T) -> Result<()>;
    fn save(&mut self) -> Result<()>;

    fn get_or_create_named(&mut self, name: &str) -> Result<T> {
        if let Some(id) = self.get_by_name(name)? {
            return Ok(id);
        }

        let id = self.try_create(T::from_byte(0), T::from_byte(0))?;
        self.set_name(id, name)?;
        self.update(id, id, id)?;
        Ok(id)
    }

    fn format_reference(&mut self, id: T) -> Result<String> {
        Ok(self
            .get_name(id)?
            .map(|name| {
                if name.parse::<T>().is_ok() && name.parse::<u32>().is_err() {
                    links_notation::encode_reference_literal(&name)
                } else {
                    escape_lino_reference(&name)
                }
            })
            .unwrap_or_else(|| id.to_string()))
    }

    fn format_lino(&mut self, link: &GenericLink<T>) -> Result<String> {
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

    fn print_change_lino(
        &mut self,
        before: &Option<GenericLink<T>>,
        after: &Option<GenericLink<T>>,
    ) -> Result<()> {
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

    fn format_structure(&mut self, id: T) -> Result<String> {
        let mut visited = HashSet::new();
        self.format_structure_recursive(id, &mut visited)
    }

    fn format_structure_recursive(&mut self, id: T, visited: &mut HashSet<T>) -> Result<String> {
        crate::structure_format::format_structure(
            self,
            id,
            visited,
            |store, address| store.get_link(address),
            |store, address| store.exists(address),
            |store, address| store.format_reference(address),
        )
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

    fn update_observed(
        &mut self,
        id: u32,
        source: u32,
        target: u32,
        observer: ChangeObserver<'_>,
    ) -> Result<Link> {
        LinkStorage::update_observed(self, id, source, target, observer)
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

    fn update_observed(
        &mut self,
        id: u32,
        source: u32,
        target: u32,
        observer: ChangeObserver<'_>,
    ) -> Result<Link> {
        NamedTypesDecorator::update_observed(self, id, source, target, observer)
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
    fn try_get_or_create(&mut self, source: u32, target: u32) -> Result<u32> {
        Ok(crate::transactions::TransactionsDecorator::get_or_create(
            self, source, target,
        )?)
    }

    fn try_create(&mut self, source: u32, target: u32) -> Result<u32> {
        Ok(crate::transactions::TransactionsDecorator::create(
            self, source, target,
        )?)
    }

    fn try_ensure_created(&mut self, id: u32) -> Result<u32> {
        validate_ensure_address(id)?;
        Ok(crate::transactions::TransactionsDecorator::ensure_created(
            self, id,
        )?)
    }

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

    fn update_observed(
        &mut self,
        id: u32,
        source: u32,
        target: u32,
        observer: ChangeObserver<'_>,
    ) -> Result<Link> {
        Ok(crate::transactions::TransactionsDecorator::update_observed(
            self, id, source, target, observer,
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
        Ok(crate::transactions::TransactionsDecorator::set_name(
            self, id, name,
        )?)
    }

    fn get_by_name(&mut self, name: &str) -> Result<Option<u32>> {
        NamedTypes::get_by_name(self.inner_mut(), name)
    }

    fn remove_name(&mut self, id: u32) -> Result<()> {
        Ok(crate::transactions::TransactionsDecorator::remove_name(
            self, id,
        )?)
    }

    fn save(&mut self) -> Result<()> {
        Ok(crate::transactions::TransactionsDecorator::save(self)?)
    }
}

impl<B: NamedTypeLinks> NamedTypeLinks
    for crate::version_control::GenericVersionControlDecorator<B>
{
    fn try_get_or_create(&mut self, source: u32, target: u32) -> Result<u32> {
        crate::version_control::GenericVersionControlDecorator::get_or_create(self, source, target)
    }

    fn try_create(&mut self, source: u32, target: u32) -> Result<u32> {
        crate::version_control::GenericVersionControlDecorator::create(self, source, target)
    }

    fn try_ensure_created(&mut self, id: u32) -> Result<u32> {
        validate_ensure_address(id)?;
        crate::version_control::GenericVersionControlDecorator::try_ensure_created(self, id)
    }

    fn create(&mut self, source: u32, target: u32) -> u32 {
        crate::version_control::GenericVersionControlDecorator::create(self, source, target)
            .expect("VersionControlDecorator::create failed in NamedTypeLinks bridge")
    }

    fn ensure_created(&mut self, id: u32) -> u32 {
        crate::version_control::GenericVersionControlDecorator::ensure_created(self, id)
    }

    fn get_link(&mut self, id: u32) -> Option<Link> {
        self.get(id).copied()
    }

    fn exists(&mut self, id: u32) -> bool {
        crate::version_control::GenericVersionControlDecorator::exists(self, id)
    }

    fn update_observed(
        &mut self,
        id: u32,
        source: u32,
        target: u32,
        observer: ChangeObserver<'_>,
    ) -> Result<Link> {
        crate::version_control::GenericVersionControlDecorator::update_observed(
            self, id, source, target, observer,
        )
    }

    fn delete_observed(&mut self, id: u32, observer: ChangeObserver<'_>) -> Result<Link> {
        crate::version_control::GenericVersionControlDecorator::delete_observed(self, id, observer)
    }

    fn all_links(&mut self) -> Vec<Link> {
        self.all().into_iter().copied().collect()
    }

    fn search(&mut self, source: u32, target: u32) -> Option<u32> {
        crate::version_control::GenericVersionControlDecorator::search(self, source, target)
    }

    fn get_or_create(&mut self, source: u32, target: u32) -> u32 {
        crate::version_control::GenericVersionControlDecorator::get_or_create(self, source, target)
            .expect("VersionControlDecorator::get_or_create failed in NamedTypeLinks bridge")
    }

    fn get_name(&mut self, id: u32) -> Result<Option<String>> {
        NamedTypes::get_name(self.transactions_mut().inner_mut(), id)
    }

    fn set_name(&mut self, id: u32, name: &str) -> Result<u32> {
        crate::version_control::GenericVersionControlDecorator::set_name(self, id, name)
    }

    fn get_by_name(&mut self, name: &str) -> Result<Option<u32>> {
        NamedTypes::get_by_name(self.transactions_mut().inner_mut(), name)
    }

    fn remove_name(&mut self, id: u32) -> Result<()> {
        crate::version_control::GenericVersionControlDecorator::remove_name(self, id)
    }

    fn save(&mut self) -> Result<()> {
        crate::version_control::GenericVersionControlDecorator::save(self)
    }
}

pub fn escape_lino_reference(reference: &str) -> String {
    if reference.is_empty() || reference.trim().is_empty() {
        return String::new();
    }

    let has_single_quote = reference.contains('\'');
    let has_double_quote = reference.contains('"');
    let needs_quoting = reference.parse::<u32>().is_ok()
        || reference.contains(':')
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
