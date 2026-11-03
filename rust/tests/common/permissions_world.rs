//! The world of `@link-foundation/ui-links`' permission tests, shared by
//! `permissions_decorator_tests.rs` and `links_sessions_tests.rs`.

#![allow(dead_code)] // Every test crate that includes it uses only part of it.

use std::collections::HashMap;

use anyhow::Result;
use link_cli::{
    AccessPolicy, Link, LinkStorage, NamedTypeLinks, PermissionAccess, PermissionMarkers,
    PermissionPolicy, PermissionsDecorator, TypeMarkers, Types, TypesDecorator,
};

/// The world of ui-links' test helpers:
///
/// ```text
///   Type -> User -> Admin              alice: User, boss: Admin
///   Type -> Doc -> Note -> Shared      Shared extends Public too
///   Type -> Doc -> Secret
///   Type -> Public
/// ```
///
/// Admins write anything, users write notes and read what is public.
pub struct World {
    pub links: LinkStorage,
    pub policy: PermissionPolicy,
    pub types: TypeMarkers,
    pub markers: PermissionMarkers,
    pub t: HashMap<&'static str, u32>,
    pub v: HashMap<&'static str, u32>,
}

pub type View<'a> = PermissionsDecorator<&'a mut LinkStorage, &'a mut PermissionPolicy>;

impl World {
    /// A new world, its rules in the data or in a store of their own.
    pub fn new(separate_rules: bool) -> Result<Self> {
        let mut links = LinkStorage::in_memory(false);
        let mut rules_apart = LinkStorage::in_memory(false);
        let (t, v, markers, types) = make_world(
            &mut links,
            if separate_rules {
                Some(&mut rules_apart)
            } else {
                None
            },
        )?;
        let policy = if separate_rules {
            PermissionPolicy::with_rules(&mut links, Box::new(rules_apart))?
        } else {
            PermissionPolicy::in_data(&mut links)?
        };
        assert_eq!(markers, policy.markers());
        assert_eq!(types, policy.types());
        Ok(Self {
            links,
            policy,
            types,
            markers,
            t,
            v,
        })
    }

    pub fn as_(&mut self, subject: u32) -> View<'_> {
        PermissionsDecorator::new(&mut self.links, &mut self.policy, subject)
    }

    pub fn rules(&mut self) -> &mut dyn NamedTypeLinks {
        self.policy.rules_of(&mut self.links)
    }

    pub fn allow(&mut self, access: PermissionAccess, subject: u32, object: u32) -> Result<u32> {
        let markers = self.markers;
        PermissionPolicy::allow(self.rules(), markers, access, subject, object)
    }

    pub fn link(&mut self, id: u32) -> Link {
        self.links.get_link(id).expect("a stored link")
    }
}

pub type Names = HashMap<&'static str, u32>;

/// The world of ui-links' test helpers in `links`, its rules in `rules` or
/// in `links` when `None`.
pub fn make_world(
    links: &mut dyn NamedTypeLinks,
    rules: Option<&mut dyn NamedTypeLinks>,
) -> Result<(Names, Names, PermissionMarkers, TypeMarkers)> {
    let mut types = TypesDecorator::with_named_markers(&mut *links)?;
    let markers = PermissionMarkers::ensure(&mut types)?;
    let mut t = HashMap::new();
    for (name, parents) in [
        ("User", &[][..]),
        ("Admin", &["User"][..]),
        ("Doc", &[][..]),
        ("Public", &[][..]),
        ("Note", &["Doc"][..]),
        ("Shared", &["Note", "Public"][..]),
        ("Secret", &["Doc"][..]),
    ] {
        let parents: Vec<u32> = parents.iter().map(|parent| t[parent]).collect();
        let id = match parents.split_first() {
            None => types.define_root()?,
            Some((parent, more)) => types.define(*parent, more)?,
        };
        types.set_name(id, name)?;
        t.insert(name, id);
    }
    let mut v = HashMap::new();
    for (name, type_name) in [
        ("alice", "User"),
        ("boss", "Admin"),
        ("note", "Note"),
        ("shared", "Shared"),
        ("secret", "Secret"),
    ] {
        let id = types.instance(t[type_name])?;
        types.set_name(id, name)?;
        v.insert(name, id);
    }
    let type_markers = types.markers();
    let rules: &mut dyn NamedTypeLinks = match rules {
        Some(rules) => rules,
        None => links,
    };
    PermissionPolicy::allow(
        rules,
        markers,
        PermissionAccess::Write,
        t["Admin"],
        markers.anything,
    )?;
    PermissionPolicy::allow(
        rules,
        markers,
        PermissionAccess::Write,
        t["User"],
        t["Note"],
    )?;
    PermissionPolicy::allow(
        rules,
        markers,
        PermissionAccess::Read,
        t["User"],
        t["Public"],
    )?;
    Ok((t, v, markers, type_markers))
}

/// Every link of `links`, by index.
pub fn all(links: &mut dyn NamedTypeLinks) -> Vec<Link> {
    let mut all = links.all_links();
    all.sort_by_key(|link| link.index);
    all
}
