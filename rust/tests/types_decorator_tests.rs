//! The type hierarchy of issue #148. The cases are the "types" tests of
//! `@link-foundation/ui-links` plus depth, diamonds and cycles; the C# twin is
//! `csharp/Foundation.Data.Doublets.Cli.Tests/TypesDecoratorTests.cs`, case
//! for case.

use std::collections::HashMap;

use anyhow::Result;
use link_cli::{
    ensure_point, Link, LinkStorage, NamedTypeLinks, NamedTypesDecorator, PinnedTypesAccess,
    PinnedTypesDecorator, Types, TypesDecorator,
};
use tempfile::NamedTempFile;

type Decorator = TypesDecorator<LinkStorage>;

fn types() -> Result<Decorator> {
    TypesDecorator::with_named_markers(LinkStorage::in_memory(false))
}

/// A world of types and values, the one of ui-links' test helpers:
///
/// ```text
///   Type -> User -> Admin              alice: User, boss: Admin
///   Type -> Doc -> Note -> Shared      Shared extends Public too
///   Type -> Doc -> Secret
///   Type -> Public
/// ```
struct World {
    types: Decorator,
    t: HashMap<&'static str, u32>,
    v: HashMap<&'static str, u32>,
}

impl World {
    fn new() -> Result<Self> {
        let mut world = Self {
            types: types()?,
            t: HashMap::new(),
            v: HashMap::new(),
        };
        world.define("User", &[])?;
        world.define("Admin", &["User"])?;
        world.define("Doc", &[])?;
        world.define("Public", &[])?;
        world.define("Note", &["Doc"])?;
        world.define("Shared", &["Note", "Public"])?;
        world.define("Secret", &["Doc"])?;
        world.instance("alice", "User")?;
        world.instance("boss", "Admin")?;
        world.instance("note", "Note")?;
        world.instance("shared", "Shared")?;
        world.instance("secret", "Secret")?;
        Ok(world)
    }

    fn define(&mut self, name: &'static str, parents: &[&str]) -> Result<()> {
        let parents: Vec<u32> = parents.iter().map(|parent| self.t[parent]).collect();
        let id = match parents.split_first() {
            None => self.types.define_root()?,
            Some((parent, more)) => self.types.define(*parent, more)?,
        };
        self.types.set_name(id, name)?;
        self.t.insert(name, id);
        Ok(())
    }

    fn instance(&mut self, name: &'static str, type_name: &str) -> Result<()> {
        let id = self.types.instance(self.t[type_name])?;
        self.types.set_name(id, name)?;
        self.v.insert(name, id);
        Ok(())
    }
}

fn link(types: &mut impl NamedTypeLinks, id: u32) -> Option<Link> {
    types.get_link(id)
}

#[test]
fn implements_named_type_links_and_types() -> Result<()> {
    fn layer<L: NamedTypeLinks + Types>(_: &L) {}
    let types = types()?;
    layer(&types);
    // Any layer can be decorated, including another decorator.
    let mut stacked = TypesDecorator::new(types, 1, Some(2));
    assert_eq!(1, stacked.root());
    assert!(stacked.links_mut().exists(1));
    Ok(())
}

#[test]
fn markers_are_named_points() -> Result<()> {
    let mut types = types()?;
    let (root, extends) = (types.root(), types.extends().unwrap());
    assert_eq!(Some(Link::new(root, root, root)), link(&mut types, root));
    assert_eq!(
        Some(Link::new(extends, extends, extends)),
        link(&mut types, extends)
    );
    assert_eq!(Some(root), types.get_by_name("Type")?);
    assert_eq!(Some(extends), types.get_by_name("Extends")?);
    // Markers that exist are found again, not created twice.
    let again = TypesDecorator::with_named_markers(types.into_inner())?;
    assert_eq!((root, Some(extends)), (again.root(), again.extends()));
    Ok(())
}

#[test]
fn follows_sub_types_and_multiple_inheritance() -> Result<()> {
    let World { mut types, t, v } = World::new()?;
    assert!(types.is_a(v["shared"], t["Doc"]));
    assert!(types.is_a(v["shared"], t["Public"]));
    assert!(!types.is_a(v["note"], t["Public"]));
    assert!(types.is_a(v["boss"], t["User"]));
    assert!(types.is_a(t["Shared"], t["Shared"]));
    assert!(!types.is_of(t["Shared"], t["Shared"]));
    assert_eq!(Some(t["Secret"]), types.type_of(v["secret"]));
    assert!(types.is_type(t["Note"]));
    assert!(!types.is_type(9999));
    assert_eq!(vec![t["Note"], t["Public"]], types.parents_of(t["Shared"]));
    assert_eq!(
        vec![t["Shared"], t["Note"], t["Public"], t["Doc"], types.root()],
        types.ancestors_of(t["Shared"])
    );
    Ok(())
}

#[test]
fn checks_a_link_that_is_not_stored_yet() -> Result<()> {
    let World { mut types, t, .. } = World::new()?;
    assert!(types.is_a_link(&Link::new(0, t["Note"], 0), t["Doc"]));
    assert!(!types.is_a_link(&Link::new(0, t["Note"], 0), t["Public"]));
    Ok(())
}

#[test]
fn needs_an_extends_marker_for_more_parents() -> Result<()> {
    let mut links = LinkStorage::in_memory(false);
    let root = ensure_point(&mut links, "Type")?;
    let mut types = TypesDecorator::new(links, root, None);
    assert_eq!(None, types.extends());
    assert!(types.define(root, &[root]).is_err());
    assert!(types.extend(root, root).is_err());
    let single = types.define(root, &[])?;
    assert_eq!(vec![root], types.parents_of(single));
    Ok(())
}

#[test]
fn acceptance_builds_the_hierarchy_as_links() -> Result<()> {
    let mut types = types()?;
    let type_id = types.root();
    let user = types.define(type_id, &[])?;
    let admin = types.define(user, &[])?;
    let doc = types.define(type_id, &[])?;
    let public = types.define(type_id, &[])?;
    let note = types.define(doc, &[])?;
    let news = types.define(note, &[doc, public])?;

    // Each definition is a link: (S: parent S), the extra parents are pairs
    // marked by Extends.
    assert_eq!(Some(Link::new(user, type_id, user)), link(&mut types, user));
    assert_eq!(Some(Link::new(admin, user, admin)), link(&mut types, admin));
    assert_eq!(Some(Link::new(news, note, news)), link(&mut types, news));
    let extends = types.extends().unwrap();
    for parent in [doc, public] {
        let pair = types.search(news, parent).expect("the pair exists");
        assert!(types.search(extends, pair).is_some());
        assert!(types.is_extension(pair));
    }

    assert!(types.is_a(news, doc));
    assert!(types.is_a(news, public));
    assert!(types.is_a(news, note));
    assert!(types.is_a(news, type_id));
    assert!(!types.is_a(news, user));
    assert!(types.is_a(admin, user));
    assert_eq!(vec![note, doc, public], types.parents_of(news));

    // A value of Note that also extends Public.
    let item = types.instance(news)?;
    assert!(types.is_a(item, doc));
    assert!(types.is_a(item, public));
    assert_eq!(Some(news), types.type_of(item));
    Ok(())
}

#[test]
fn follows_depth_three_and_more() -> Result<()> {
    let mut types = types()?;
    let a = types.define_root()?;
    let b = types.define(a, &[])?;
    let c = types.define(b, &[])?;
    let d = types.define(c, &[])?;
    let e = types.define(d, &[])?;
    let value = types.instance(e)?;
    for type_id in [e, d, c, b, a, types.root()] {
        assert!(types.is_a(value, type_id));
        assert!(types.is_of(value, type_id));
    }
    assert!(!types.is_a(a, b));
    assert_eq!(vec![e, d, c, b, a, types.root()], types.ancestors_of(e));
    Ok(())
}

#[test]
fn visits_a_diamond_once() -> Result<()> {
    let mut types = types()?;
    let top = types.define_root()?;
    let left = types.define(top, &[])?;
    let right = types.define(top, &[])?;
    let bottom = types.define(left, &[right])?;
    let value = types.instance(bottom)?;
    assert!(types.is_a(value, top));
    assert!(types.is_a(value, left));
    assert!(types.is_a(value, right));
    assert_eq!(
        vec![bottom, left, right, top, types.root()],
        types.ancestors_of(bottom)
    );
    Ok(())
}

#[test]
fn terminates_on_an_extends_cycle() -> Result<()> {
    let mut types = types()?;
    // A extends B and B extends A.
    let a = types.define_root()?;
    let b = types.define_root()?;
    types.extend(a, b)?;
    types.extend(b, a)?;
    let unrelated = types.define_root()?;
    let value_of_a = types.instance(a)?;
    let value_of_b = types.instance(b)?;

    assert_eq!(vec![types.root(), b], types.parents_of(a));
    assert_eq!(vec![types.root(), a], types.parents_of(b));
    assert!(types.is_a(value_of_a, b));
    assert!(types.is_a(value_of_b, a));
    assert!(types.is_of(a, b));
    assert!(types.is_of(b, a));
    assert!(!types.is_a(value_of_a, unrelated));
    // Through the cycle a type is strictly of itself.
    assert!(types.is_of(a, a));
    assert_eq!(vec![a, types.root(), b], types.ancestors_of(a));
    assert_eq!(vec![b, types.root(), a], types.ancestors_of(b));
    Ok(())
}

#[test]
fn refuses_to_extend_a_direct_sub_type() -> Result<()> {
    let mut types = types()?;
    let a = types.define_root()?;
    let b = types.define(a, &[])?;
    // (b: a b) is the doublet (a b) itself.
    assert!(types.extend(a, b).is_err());
    assert!(!types.is_extension(b));
    assert_eq!(vec![a], types.parents_of(b));
    assert_eq!(vec![types.root()], types.parents_of(a));
    Ok(())
}

#[test]
fn terminates_on_a_source_cycle() -> Result<()> {
    let mut types = types()?;
    let a = types.define_root()?;
    let b = types.define(a, &[])?;
    // (a: b a) and (b: a b): each is a sub-type of the other.
    types.update(a, b, a)?;
    let unrelated = types.define_root()?;

    assert_eq!(vec![b], types.parents_of(a));
    assert_eq!(vec![a], types.parents_of(b));
    assert!(types.is_of(a, b));
    assert!(!types.is_of(a, types.root()));
    let value = types.instance(a)?;
    assert!(!types.is_a(value, unrelated));
    assert_eq!(vec![a, b], types.ancestors_of(a));
    Ok(())
}

#[test]
fn wiring_links_are_of_type_extends() -> Result<()> {
    let World { mut types, t, .. } = World::new()?;
    let extends = types.extends().unwrap();
    let pair = types.search(t["Shared"], t["Public"]).unwrap();
    let mark = types.search(extends, pair).unwrap();
    assert!(types.is_extension(pair));
    assert!(!types.is_extension(mark));
    assert!(!types.is_extension(t["Shared"]));
    assert!(types.is_a(pair, extends));
    assert!(types.is_a(mark, extends));
    assert!(!types.is_a(pair, t["Shared"]));
    assert!(!types.is_a(pair, t["Doc"]));
    assert_eq!(Some(extends), types.type_of(mark));
    Ok(())
}

#[test]
fn instances_with_a_target() -> Result<()> {
    let World { mut types, t, v } = World::new()?;
    let membership = types.instance_with_target(t["User"], v["boss"])?;
    assert_eq!(
        Some(Link::new(membership, t["User"], v["boss"])),
        link(&mut types, membership)
    );
    assert_eq!(Some(t["User"]), types.type_of(membership));
    assert!(!types.is_type(membership));
    assert!(types.is_a(membership, t["User"]));
    assert!(types.is_a(membership, types.root()));
    // The same doublet is the same value.
    assert_eq!(
        membership,
        types.instance_with_target(t["User"], v["boss"])?
    );
    Ok(())
}

#[test]
fn answers_for_missing_links() -> Result<()> {
    let World { mut types, .. } = World::new()?;
    assert_eq!(None, types.type_of(9999));
    assert!(!types.is_type(9999));
    assert!(types.parents_of(9999).is_empty());
    assert_eq!(vec![9999], types.ancestors_of(9999));
    assert!(!types.is_a(9999, 9999));
    assert!(!types.is_of(9999, types.root()));
    let root = types.root();
    let missing = types.define(9999, &[]).unwrap_err();
    assert_eq!("Link 9999 does not exist.", missing.to_string());
    assert!(types.define(root, &[9999]).is_err());
    assert!(types.instance(9999).is_err());
    Ok(())
}

#[test]
fn pinned_types_are_sub_types_of_the_root() -> Result<()> {
    let mut pinned = PinnedTypesDecorator::from_link_storage(LinkStorage::in_memory(false));
    let (type_id, unicode_symbol, unicode_sequence) = pinned.deconstruct_pinned_types()?;
    assert_eq!((1, 2, 3), (type_id, unicode_symbol, unicode_sequence));

    let extends = pinned.create(0, 0);
    pinned.update(extends, extends, extends)?;
    let mut types = TypesDecorator::new(pinned.into_link_storage(), type_id, Some(extends));

    assert!(types.is_type(type_id));
    assert!(types.parents_of(type_id).is_empty());
    for pinned_type in [unicode_symbol, unicode_sequence] {
        assert!(types.is_type(pinned_type));
        assert_eq!(Some(type_id), types.type_of(pinned_type));
        assert_eq!(vec![type_id], types.parents_of(pinned_type));
        assert!(types.is_of(pinned_type, type_id));
    }
    let text = types.define(unicode_sequence, &[unicode_symbol])?;
    let value = types.instance(text)?;
    assert!(types.is_a(value, type_id));
    assert_eq!(
        vec![text, unicode_sequence, unicode_symbol, type_id],
        types.ancestors_of(text)
    );
    // The pinned types still enumerate as before.
    let mut pinned = PinnedTypesDecorator::from_link_storage(types.into_inner());
    assert_eq!(vec![1, 2, 3], pinned.pinned_types(3)?);
    Ok(())
}

/// The hierarchy is plain links, so a file-backed store keeps it
/// across a reopen and the named markers are found again.
#[test]
fn keeps_the_hierarchy_in_a_named_file_store() -> Result<()> {
    let file = NamedTempFile::new()?;
    let (note, news, public) = {
        let mut types =
            TypesDecorator::with_named_markers(NamedTypesDecorator::new(file.path(), false)?)?;
        let doc = types.define_root()?;
        let public = types.define_root()?;
        let note = types.define(doc, &[])?;
        let news = types.define(note, &[public])?;
        types.set_name(public, "Public")?;
        types.save()?;
        (note, news, public)
    };
    let mut types =
        TypesDecorator::with_named_markers(NamedTypesDecorator::new(file.path(), false)?)?;
    assert_eq!(Some(public), types.get_by_name("Public")?);
    assert_eq!(vec![note, public], types.parents_of(news));
    let item = types.instance(news)?;
    assert!(types.is_a(item, public));
    Ok(())
}
