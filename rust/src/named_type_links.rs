use anyhow::{Context, Result};
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::path::Path;

use crate::error::LinkError;
use crate::link::Link;
use crate::link_storage::LinkStorage;
use crate::named_types::{NamedTypes, NamedTypesDecorator};

pub trait NamedTypeLinks {
    fn create(&mut self, source: u32, target: u32) -> u32;
    fn ensure_created(&mut self, id: u32) -> u32;
    fn get_link(&mut self, id: u32) -> Option<Link>;
    fn exists(&mut self, id: u32) -> bool;
    fn update(&mut self, id: u32, source: u32, target: u32) -> Result<Link>;
    fn delete(&mut self, id: u32) -> Result<Link>;
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
        let link = self.get_link(id).ok_or(LinkError::NotFound(id))?;
        self.format_structure_recursive(&link, true)
    }

    fn format_structure_recursive(&mut self, link: &Link, is_root: bool) -> Result<String> {
        if link.is_full_point() && !is_root {
            return self.format_reference(link.index);
        }

        let source = if link.source == link.index {
            self.format_reference(link.index)?
        } else if let Some(source_link) = self.get_link(link.source) {
            self.format_structure_recursive(&source_link, false)?
        } else {
            link.source.to_string()
        };

        let target = if link.target == link.index {
            self.format_reference(link.index)?
        } else if let Some(target_link) = self.get_link(link.target) {
            self.format_structure_recursive(&target_link, false)?
        } else {
            link.target.to_string()
        };

        Ok(format!("({source} {target})"))
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

pub(crate) fn escape_lino_reference(reference: &str) -> String {
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
