//! A type hierarchy with multiple inheritance kept in plain links (issue #148).
//!
//! Run it with:
//!
//! ```text
//! cargo run --manifest-path rust/Cargo.toml --example type_hierarchy
//! ```
//!
//! It builds `Type -> Doc -> Note -> News`, makes `News` extend `Public`
//! too, and asks what a value of `News` is. Every step is an ordinary link,
//! so the store prints as LiNo at the end.

use anyhow::Result;
use link_cli::{LinkStorage, NamedTypeLinks, Types, TypesDecorator};

type Hierarchy = TypesDecorator<LinkStorage>;

fn define(types: &mut Hierarchy, name: &str, parent: u32, more: &[u32]) -> Result<u32> {
    let id = types.define(parent, more)?;
    types.set_name(id, name)?;
    Ok(id)
}

fn names(types: &mut Hierarchy, ids: Vec<u32>) -> Result<String> {
    Ok(ids
        .into_iter()
        .map(|id| types.format_reference(id))
        .collect::<Result<Vec<_>>>()?
        .join(" "))
}

fn main() -> Result<()> {
    // The points named Type and Extends are created when missing.
    let mut types = TypesDecorator::with_named_markers(LinkStorage::in_memory(false))?;
    let root = types.root();

    let user = define(&mut types, "User", root, &[])?;
    let doc = define(&mut types, "Doc", root, &[])?;
    let public = define(&mut types, "Public", root, &[])?;
    let note = define(&mut types, "Note", doc, &[])?;
    // News is a sub-type of Note and extends Doc and Public too.
    let news = define(&mut types, "News", note, &[doc, public])?;

    let today = types.instance(news)?;
    types.set_name(today, "today")?;

    for (name, id) in [("Doc", doc), ("Public", public), ("User", user)] {
        println!("is_a(today, {name}) = {}", types.is_a(today, id));
    }
    let parents = types.parents_of(news);
    println!("parents_of(News) = {}", names(&mut types, parents)?);
    let ancestors = types.ancestors_of(news);
    println!("ancestors_of(News) = {}", names(&mut types, ancestors)?);

    println!();
    types.print_all_lino()
}
