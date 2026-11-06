//! An optional permission layer with permissions stored in links (issue #149),
//! the Rust port of `createPolicy` and `withPermissions` of
//! `@link-foundation/ui-links`; the C# counterparts are
//! `PermissionPolicy<TLinkAddress>` and `PermissionsDecorator<TLinkAddress>`.
//!
//! A rule is a pair `(g: Subject Object)` marked as `(r: AllowRead g)` or
//! `(r: AllowWrite g)`: the subjects that are `Subject` (the subject itself,
//! or a link of that type) may read, or read and write, the links of type
//! `Object` -- its values and sub-types, not the link of `Object` itself.
//! `Anything` in either place matches every subject or every link. Types are
//! those of [`Types`](crate::types::Types) (issue #148).
//!
//! Referring to a link is a right of its own (issue #161). Without it an
//! address the subject may not read answers `Link not found`, exactly as a
//! missing one. A subject may refer to an address when it may read the link
//! there, when it is that link, when a rule `(r: AllowReference (g: Subject
//! Object))` covers it (`Object` itself and the links of type `Object`), or
//! when it is the `Object` of an `AllowRead` or `AllowWrite` rule of the
//! subject, so `User writes Note` is enough to type a link as `Note`.
//! `AllowReference` does not allow reading. A rule from `Anything` to
//! `Anything` lets every subject refer to every link.
//!
//! The markers are points of the data, so every address a rule holds is an
//! address of the data. The rules live in the data itself or in a store of
//! their own. Rules in the data are links like any other: a subject that may
//! write them may change the rules, so give `Anything` writes to
//! administrators only, or keep the rules apart.

mod tokens;

pub use tokens::LinksTokenTable;

use std::collections::{HashMap, HashSet};

use anyhow::{bail, Result};
use thiserror::Error;

use crate::error::LinkError;
use crate::link::Link;
use crate::link_storage::ChangeObserver;
use crate::named_type_links::NamedTypeLinks;
use crate::types::{ensure_point, TypeMarkers, Types, TypesDecorator};
use crate::undoable_links::{keep_names_before_update, LinksUndoLog};

/// A write a subject may not make; it reads `Forbidden: what`.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("Forbidden: {what}")]
pub struct Forbidden {
    /// What was refused, such as `update 7`.
    pub what: String,
}

impl Forbidden {
    pub fn new(what: impl Into<String>) -> Self {
        Self { what: what.into() }
    }
}

/// What a rule allows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PermissionAccess {
    /// Reading: the link is seen.
    Read,
    /// Reading and writing: the link may be created, updated, deleted and named.
    Write,
    /// Referring: the link may be used in another link, not seen.
    Reference,
}

impl PermissionAccess {
    /// Parses `read`, `write` or `reference`.
    pub fn parse(access: &str) -> Result<Self> {
        match access {
            "read" => Ok(Self::Read),
            "write" => Ok(Self::Write),
            "reference" => Ok(Self::Reference),
            _ => bail!("Unknown access: {access}"),
        }
    }
}

/// The four points of the data permission rules are made of: the markers
/// `AllowRead`, `AllowWrite` and `AllowReference` of rules and `Anything`, which
/// matches every subject or every link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PermissionMarkers {
    pub allow_read: u32,
    pub allow_write: u32,
    pub anything: u32,
    pub allow_reference: u32,
}

impl PermissionMarkers {
    /// The name of the marker of read rules.
    pub const ALLOW_READ_NAME: &'static str = "AllowRead";
    /// The name of the marker of write rules.
    pub const ALLOW_WRITE_NAME: &'static str = "AllowWrite";
    /// The name of the marker that matches every subject or link.
    pub const ANYTHING_NAME: &'static str = "Anything";
    /// The name of the marker of reference rules.
    pub const ALLOW_REFERENCE_NAME: &'static str = "AllowReference";

    /// The points named `AllowRead`, `AllowWrite`, `Anything` and
    /// `AllowReference`, each created as a point `(x: x x)` and named when
    /// missing. `AllowReference` comes last, so the others keep their addresses
    /// in a store made before it was added.
    pub fn ensure<L: NamedTypeLinks + ?Sized>(links: &mut L) -> Result<Self> {
        Ok(Self {
            allow_read: ensure_point(links, Self::ALLOW_READ_NAME)?,
            allow_write: ensure_point(links, Self::ALLOW_WRITE_NAME)?,
            anything: ensure_point(links, Self::ANYTHING_NAME)?,
            allow_reference: ensure_point(links, Self::ALLOW_REFERENCE_NAME)?,
        })
    }

    /// The marker of the rules of `access`.
    pub fn of(&self, access: PermissionAccess) -> u32 {
        match access {
            PermissionAccess::Read => self.allow_read,
            PermissionAccess::Write => self.allow_write,
            PermissionAccess::Reference => self.allow_reference,
        }
    }
}

/// Who may read, write and refer to what.
///
/// A subject is a link address; the null link (0) is a guest, which no rule
/// but one for `Anything` matches. A link is given as `(index, source,
/// target)`, so a link that is not stored yet, or not any more, can be asked
/// about too. `data` is the store the links are in, for their types.
pub trait AccessPolicy {
    /// The markers the rules are made of.
    fn markers(&self) -> PermissionMarkers;

    /// Whether `subject` may read `link`; writing allows reading.
    fn can_read(&mut self, data: &mut dyn NamedTypeLinks, subject: u32, link: &Link) -> bool;

    /// Whether `subject` may write `link`, as it is or as it will be.
    fn can_write(&mut self, data: &mut dyn NamedTypeLinks, subject: u32, link: &Link) -> bool;

    /// Whether `subject` may write anything at all, and so create links.
    fn can_create(&mut self, data: &mut dyn NamedTypeLinks, subject: u32) -> bool;

    /// Whether `subject` may refer to `link` although it need not read it. A
    /// missing address is asked about as an untyped link. A policy without
    /// this right allows references to readable links only.
    fn can_reference(
        &mut self,
        _data: &mut dyn NamedTypeLinks,
        _subject: u32,
        _link: &Link,
    ) -> bool {
        false
    }
}

impl<P: AccessPolicy + ?Sized> AccessPolicy for &mut P {
    fn markers(&self) -> PermissionMarkers {
        (**self).markers()
    }

    fn can_read(&mut self, data: &mut dyn NamedTypeLinks, subject: u32, link: &Link) -> bool {
        (**self).can_read(data, subject, link)
    }

    fn can_write(&mut self, data: &mut dyn NamedTypeLinks, subject: u32, link: &Link) -> bool {
        (**self).can_write(data, subject, link)
    }

    fn can_create(&mut self, data: &mut dyn NamedTypeLinks, subject: u32) -> bool {
        (**self).can_create(data, subject)
    }

    fn can_reference(&mut self, data: &mut dyn NamedTypeLinks, subject: u32, link: &Link) -> bool {
        (**self).can_reference(data, subject, link)
    }
}

impl<P: AccessPolicy + ?Sized> AccessPolicy for Box<P> {
    fn markers(&self) -> PermissionMarkers {
        (**self).markers()
    }

    fn can_read(&mut self, data: &mut dyn NamedTypeLinks, subject: u32, link: &Link) -> bool {
        (**self).can_read(data, subject, link)
    }

    fn can_write(&mut self, data: &mut dyn NamedTypeLinks, subject: u32, link: &Link) -> bool {
        (**self).can_write(data, subject, link)
    }

    fn can_create(&mut self, data: &mut dyn NamedTypeLinks, subject: u32) -> bool {
        (**self).can_create(data, subject)
    }

    fn can_reference(&mut self, data: &mut dyn NamedTypeLinks, subject: u32, link: &Link) -> bool {
        (**self).can_reference(data, subject, link)
    }
}

/// Permissions stored as links; see the [module documentation](self).
pub struct PermissionPolicy {
    markers: PermissionMarkers,
    types: TypeMarkers,
    rules: Option<Box<dyn NamedTypeLinks>>,
}

impl PermissionPolicy {
    /// The policy of the rules marked by `markers`, with the types of
    /// `types`: in the data when `rules` is `None`, else in `rules`.
    pub fn new(
        markers: PermissionMarkers,
        types: TypeMarkers,
        rules: Option<Box<dyn NamedTypeLinks>>,
    ) -> Self {
        Self {
            markers,
            types,
            rules,
        }
    }

    /// The policy of the rules in `data`, whose points named `Type`,
    /// `Extends`, `AllowRead`, `AllowWrite`, `Anything` and `AllowReference`
    /// are created when missing.
    pub fn in_data<L: NamedTypeLinks + ?Sized>(data: &mut L) -> Result<Self> {
        let types = TypeMarkers::ensure(data)?;
        let markers = PermissionMarkers::ensure(data)?;
        Ok(Self::new(markers, types, None))
    }

    /// The policy of the rules in `rules`, a store of their own, over `data`,
    /// whose points are created when missing as by [`in_data`](Self::in_data).
    pub fn with_rules<L: NamedTypeLinks + ?Sized>(
        data: &mut L,
        rules: Box<dyn NamedTypeLinks>,
    ) -> Result<Self> {
        let types = TypeMarkers::ensure(data)?;
        let markers = PermissionMarkers::ensure(data)?;
        Ok(Self::new(markers, types, Some(rules)))
    }

    /// The type markers of the data.
    pub fn types(&self) -> TypeMarkers {
        self.types
    }

    /// The store of the rules when they are kept apart from the data.
    pub fn rules_mut(&mut self) -> Option<&mut (dyn NamedTypeLinks + 'static)> {
        self.rules.as_deref_mut()
    }

    /// The store of the rules: `data` itself, or the rules' own store.
    pub fn rules_of<'a>(
        &'a mut self,
        data: &'a mut dyn NamedTypeLinks,
    ) -> &'a mut dyn NamedTypeLinks {
        match self.rules.as_deref_mut() {
            Some(rules) => rules,
            None => data,
        }
    }

    /// The rules `(Subject Object)` that `marker` marks, in the order of their marks.
    pub fn granted(&mut self, data: &mut dyn NamedTypeLinks, marker: u32) -> Vec<(u32, u32)> {
        let rules = self.rules_of(data);
        let mut marks: Vec<(u32, u32)> = rules
            .query_links(None, Some(marker), None)
            .into_iter()
            // The marker point (x: x x) itself is no mark; in a store of rules
            // of their own a mark may sit at the marker's address.
            .filter(|mark| mark.index != marker || mark.target != marker)
            .map(|mark| (mark.index, mark.target))
            .collect();
        marks.sort_by_key(|&(mark, _)| mark);
        marks
            .into_iter()
            .filter_map(|(_, rule)| {
                if rule == 0 {
                    return None;
                }
                rules.get_link(rule).map(|link| (link.source, link.target))
            })
            .collect()
    }

    /// Whether a rule of `marker` lets `subject` at `link`.
    pub fn allows(
        &mut self,
        data: &mut dyn NamedTypeLinks,
        marker: u32,
        subject: u32,
        link: &Link,
    ) -> bool {
        let granted = self.granted(data, marker);
        if granted.is_empty() {
            return false;
        }
        let anything = self.markers.anything;
        let mut types = TypesDecorator::with_markers(data, self.types);
        granted.into_iter().any(|(rule_subject, rule_object)| {
            is_subject(&mut types, anything, subject, rule_subject)
                && (rule_object == anything || types.is_of_link(link, rule_object))
        })
    }

    /// Writes a rule into `rules`: `subject` may `access` the links of
    /// `object`. The rule is the pair `(subject object)` marked by `(marker
    /// rule)`; returns the pair.
    pub fn allow<L: NamedTypeLinks + ?Sized>(
        rules: &mut L,
        markers: PermissionMarkers,
        access: PermissionAccess,
        subject: u32,
        object: u32,
    ) -> Result<u32> {
        if subject == markers.allow_read
            || subject == markers.allow_write
            || subject == markers.allow_reference
        {
            // A pair from a marker would read as a mark.
            bail!("A marker cannot be a subject");
        }
        // A rule given twice is the same rule: the pair and its mark are
        // found before they are made.
        let rule = rules.try_get_or_create(subject, object)?;
        rules.try_get_or_create(markers.of(access), rule)?;
        Ok(rule)
    }

    /// Parses `read`, `write` or `reference`.
    pub fn parse_access(access: &str) -> Result<PermissionAccess> {
        PermissionAccess::parse(access)
    }
}

/// Whether `subject` is `type_id` or of it; a guest is only `Anything`.
fn is_subject<T: Types>(types: &mut T, anything: u32, subject: u32, type_id: u32) -> bool {
    type_id == anything || (subject != 0 && types.is_a(subject, type_id))
}

impl AccessPolicy for PermissionPolicy {
    fn markers(&self) -> PermissionMarkers {
        self.markers
    }

    fn can_read(&mut self, data: &mut dyn NamedTypeLinks, subject: u32, link: &Link) -> bool {
        let markers = self.markers;
        self.allows(data, markers.allow_read, subject, link)
            || self.allows(data, markers.allow_write, subject, link)
    }

    fn can_write(&mut self, data: &mut dyn NamedTypeLinks, subject: u32, link: &Link) -> bool {
        let marker = self.markers.allow_write;
        self.allows(data, marker, subject, link)
    }

    fn can_create(&mut self, data: &mut dyn NamedTypeLinks, subject: u32) -> bool {
        let marker = self.markers.allow_write;
        let granted = self.granted(data, marker);
        let anything = self.markers.anything;
        let mut types = TypesDecorator::with_markers(data, self.types);
        granted
            .into_iter()
            .any(|(rule_subject, _)| is_subject(&mut types, anything, subject, rule_subject))
    }

    fn can_reference(&mut self, data: &mut dyn NamedTypeLinks, subject: u32, link: &Link) -> bool {
        let markers = self.markers;
        if (subject != 0 && link.index == subject)
            || self.allows(data, markers.allow_reference, subject, link)
        {
            return true;
        }
        // The object of a rule of the subject is named by that rule: a readable
        // link of that type shows its address anyway.
        [
            markers.allow_read,
            markers.allow_write,
            markers.allow_reference,
        ]
        .into_iter()
        .any(|marker| {
            let granted = self.granted(data, marker);
            let mut types = TypesDecorator::with_markers(&mut *data, self.types);
            granted.into_iter().any(|(rule_subject, rule_object)| {
                rule_object == link.index
                    && is_subject(&mut types, markers.anything, subject, rule_subject)
            })
        })
    }
}

/// The store as one subject sees it; see the [module documentation](self).
///
/// Reads skip the links the subject may not read, so a hidden link is as
/// missing as a deleted one: to [`exists`](NamedTypeLinks::exists), to
/// [`get_link`](NamedTypeLinks::get_link), and to every write, which answers
/// `Link not found`. A write the subject may not make fails with
/// [`Forbidden`] and changes nothing. An update needs the right to write the
/// link before and after it.
///
/// A link the subject created through this view is its own while it has no
/// type (its source is 0), so it can give it one. [`forget`](Self::forget)
/// drops that claim when the link is gone.
///
/// The store below may change more than the link asked for: deleting a link
/// deletes the links that use it, and an update that makes a link equal to
/// another merges them, repointing the usages. Every link a write changes
/// must be the subject's to change; when one is not, the write is undone and
/// refused as a whole.
///
/// A reference needs [`can_reference`](Self::can_reference): without the right
/// an unreadable address answers as a missing one, to
/// [`reference_exists`](NamedTypeLinks::reference_exists), to a create and to
/// an update. The write check runs first, so a type the subject may not write
/// stays [`Forbidden`] for hidden and missing alike. A reference a link already
/// holds may stay; once replaced, it cannot be put back without the right.
/// Choosing the address of a new link
/// ([`try_ensure_created`](NamedTypeLinks::try_ensure_created)) is a reference
/// too, so a limited subject cannot choose addresses.
///
/// Names follow references: [`get_name`](NamedTypeLinks::get_name) and
/// [`get_by_name`](NamedTypeLinks::get_by_name) answer only for a link the
/// subject may refer to, so any other name is an unknown name. Naming or
/// unnaming a link needs the right to write it, and taking a name needs the
/// right to write the link that holds it.
pub struct PermissionsDecorator<L: NamedTypeLinks, P: AccessPolicy = PermissionPolicy> {
    links: L,
    policy: P,
    subject: u32,
    created: HashSet<u32>,
}

impl<L: NamedTypeLinks, P: AccessPolicy> PermissionsDecorator<L, P> {
    /// `links` as `subject` (the null link is a guest) sees it under `policy`.
    pub fn new(links: L, policy: P, subject: u32) -> Self {
        Self {
            links,
            policy,
            subject,
            created: HashSet::new(),
        }
    }

    /// The same view with the links `created` before taken as the subject's own.
    pub fn with_created(mut self, created: HashSet<u32>) -> Self {
        self.created = created;
        self
    }

    /// The subject the store is seen as.
    pub fn subject(&self) -> u32 {
        self.subject
    }

    pub fn set_subject(&mut self, subject: u32) {
        self.subject = subject;
    }

    /// The links the subject created through this view and still has.
    pub fn created(&self) -> &HashSet<u32> {
        &self.created
    }

    /// Swaps the links taken as the subject's own, returning the old ones.
    pub fn replace_created(&mut self, created: HashSet<u32>) -> HashSet<u32> {
        std::mem::replace(&mut self.created, created)
    }

    pub fn policy(&self) -> &P {
        &self.policy
    }

    pub fn policy_mut(&mut self) -> &mut P {
        &mut self.policy
    }

    pub fn inner(&self) -> &L {
        &self.links
    }

    pub fn inner_mut(&mut self) -> &mut L {
        &mut self.links
    }

    pub fn into_inner(self) -> L {
        self.links
    }

    /// Whether `link` is the subject's own: created through this view and
    /// still without a type.
    pub fn owns(&self, link: &Link) -> bool {
        link.source == 0 && self.created.contains(&link.index)
    }

    /// Whether the subject may read `link`.
    pub fn can_read(&mut self, link: &Link) -> bool {
        self.owns(link) || self.policy.can_read(&mut self.links, self.subject, link)
    }

    /// Whether the subject may write `link` as it is.
    pub fn can_write(&mut self, link: &Link) -> bool {
        self.owns(link) || self.policy.can_write(&mut self.links, self.subject, link)
    }

    /// Whether the subject may refer to the address `id`, which may hold no
    /// link: a missing address is judged as an untyped link, so only a rule
    /// for `Anything` covers it.
    pub fn can_reference(&mut self, id: u32) -> bool {
        let link = self.links.get_link(id).unwrap_or(Link::new(id, 0, 0));
        self.owns_triplet(&link)
            || self.can_read(&link)
            || self
                .policy
                .can_reference(&mut self.links, self.subject, &link)
    }

    /// The references `before` does not hold yet need the right: without it
    /// the address answers as a missing one.
    fn check_references(&mut self, before: Option<&Link>, source: u32, target: u32) -> Result<()> {
        for part in [source, target] {
            let held = before.is_some_and(|link| {
                part == link.index || part == link.source || part == link.target
            });
            if part != 0 && !held && !self.can_reference(part) {
                return Err(LinkError::not_found(part).into());
            }
        }
        Ok(())
    }

    /// [`owns`](Self::owns) when the store is read as triplets, where a point
    /// that is only its own type has no type yet either.
    pub(crate) fn owns_triplet(&self, link: &Link) -> bool {
        let point = link.source == link.index && link.target == link.index;
        self.owns(link) || (point && self.created.contains(&link.index))
    }

    /// [`can_read`](Self::can_read) when the store is read as triplets.
    pub(crate) fn can_read_triplet(&mut self, link: &Link) -> bool {
        self.owns_triplet(link) || self.policy.can_read(&mut self.links, self.subject, link)
    }

    /// [`can_write`](Self::can_write) when the store is read as triplets.
    pub(crate) fn can_write_triplet(&mut self, link: &Link) -> bool {
        self.owns_triplet(link) || self.policy.can_write(&mut self.links, self.subject, link)
    }

    /// Whether the policy lets the subject write `link`, ownership aside.
    pub(crate) fn policy_can_write(&mut self, link: &Link) -> bool {
        self.policy.can_write(&mut self.links, self.subject, link)
    }

    pub(crate) fn policy_can_create(&mut self) -> bool {
        self.policy.can_create(&mut self.links, self.subject)
    }

    /// The link at `index` is one the subject made on the store below.
    pub(crate) fn remember(&mut self, index: u32) {
        self.created.insert(index);
    }

    /// The store below and the policy, to judge a request on a layer over the
    /// same store.
    pub(crate) fn parts_mut(&mut self) -> (&mut L, &mut P) {
        (&mut self.links, &mut self.policy)
    }

    /// The link at `index` is gone: a new one there is not the subject's own.
    pub fn forget(&mut self, index: u32) {
        self.created.remove(&index);
    }

    /// The stored link at `index`, `None` when there is none; a hidden one is
    /// not found.
    fn visible(&mut self, index: u32) -> Result<Option<Link>> {
        if index == 0 {
            return Ok(None);
        }
        let Some(link) = self.links.get_link(index) else {
            return Ok(None);
        };
        if !self.can_read(&link) {
            return Err(LinkError::not_found(index).into());
        }
        Ok(Some(link))
    }

    fn require_writable(&mut self, index: u32, what: &str) -> Result<()> {
        let link = self
            .visible(index)?
            .ok_or_else(|| anyhow::Error::from(LinkError::not_found(index)))?;
        if !self.can_write(&link) {
            return Err(Forbidden::new(format!("{what} {index}")).into());
        }
        Ok(())
    }

    fn require_create(&mut self, source: u32, target: u32) -> Result<()> {
        if !self.policy_can_create()
            || (source != 0 && !self.policy_can_write(&Link::new(0, source, target)))
        {
            return Err(Forbidden::new("create").into());
        }
        Ok(())
    }

    /// Whether the subject may change a link from `before` to `after`.
    fn allowed(&mut self, before: Option<Link>, after: Option<Link>) -> bool {
        match (before, after) {
            (Some(was), Some(now)) if was == now => true,
            (None, made) => {
                self.policy_can_create()
                    && made.is_none_or(|made| made.source == 0 || self.policy_can_write(&made))
            }
            (Some(was), None) => self.can_write(&was),
            (Some(was), Some(now)) => {
                (self.owns(&was) && now.source == 0)
                    || (self.can_write(&was) && self.policy_can_write(&now))
            }
        }
    }

    /// Makes a write on the store below, then checks every link it changed:
    /// the link asked for is checked already, any other must be the
    /// subject's to change, or the write is undone and refused.
    fn checked(
        &mut self,
        what: &str,
        index: u32,
        mut log: LinksUndoLog,
        write: impl FnOnce(&mut L, ChangeObserver<'_>) -> Result<Link>,
        observer: ChangeObserver<'_>,
    ) -> Result<Link> {
        let result = write(&mut self.links, &mut |before, after| {
            log.record_observed(before, after)
        });
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                if let Err(undo) = self.undo(&mut log) {
                    return Err(error.context(format!("and undoing it failed: {undo}")));
                }
                return Err(error);
            }
        };
        let mut net: HashMap<u32, (Option<Link>, Option<Link>)> = HashMap::new();
        let mut order = Vec::new();
        for &(before, after) in log.changes() {
            let Some(at) = before.or(after).map(|link| link.index) else {
                continue;
            };
            match net.get_mut(&at) {
                Some(known) => known.1 = after,
                None => {
                    net.insert(at, (before, after));
                    order.push(at);
                }
            }
        }
        for &at in &order {
            let (first, last) = net[&at];
            if at == index || self.allowed(first, last) {
                continue;
            }
            self.undo(&mut log)?;
            return Err(Forbidden::new(what).into());
        }
        for &at in &order {
            if net[&at].1.is_none() {
                self.created.remove(&at);
            }
        }
        for &(before, after) in log.changes() {
            observer(
                before.unwrap_or(Link::null()),
                after.unwrap_or(Link::null()),
            );
        }
        Ok(result)
    }

    fn undo(&mut self, log: &mut LinksUndoLog) -> Result<()> {
        for gone in log.undo(&mut self.links)? {
            self.created.remove(&gone);
        }
        Ok(())
    }

    fn panic_on<T>(result: Result<T>) -> T {
        result.unwrap_or_else(|error| panic!("{error}"))
    }
}

impl<L: NamedTypeLinks, P: AccessPolicy> NamedTypeLinks for PermissionsDecorator<L, P> {
    /// Panics when the subject may not create; use
    /// [`try_create`](NamedTypeLinks::try_create).
    fn create(&mut self, source: u32, target: u32) -> u32 {
        Self::panic_on(self.try_create(source, target))
    }

    /// Panics when the subject may not create; use
    /// [`try_ensure_created`](NamedTypeLinks::try_ensure_created).
    fn ensure_created(&mut self, id: u32) -> u32 {
        Self::panic_on(self.try_ensure_created(id))
    }

    fn try_ensure_created(&mut self, id: u32) -> Result<u32> {
        if id != 0 && self.exists(id) {
            return Ok(id);
        }
        if !self.policy_can_create() {
            return Err(Forbidden::new("create").into());
        }
        if id != 0 && !self.can_reference(id) {
            return Err(LinkError::not_found(id).into());
        }
        if id != 0 && self.links.exists(id) {
            // The subject may refer to this hidden link: the point must not land on it.
            bail!("Link {id} exists");
        }
        let created = self.links.try_ensure_created(id)?;
        if id != 0 {
            self.created.insert(created);
        }
        Ok(created)
    }

    fn get_link(&mut self, id: u32) -> Option<Link> {
        let link = self.links.get_link(id)?;
        self.can_read(&link).then_some(link)
    }

    fn exists(&mut self, id: u32) -> bool {
        self.get_link(id).is_some()
    }

    fn try_create(&mut self, source: u32, target: u32) -> Result<u32> {
        self.require_create(source, target)?;
        self.check_references(None, source, target)?;
        let id = self.links.try_create(source, target)?;
        self.created.insert(id);
        Ok(id)
    }

    /// A link the subject may not refer to is as missing as a deleted one.
    fn reference_exists(&mut self, id: u32) -> bool {
        self.links.reference_exists(id) && self.can_reference(id)
    }

    fn claims(&mut self, id: u32) -> Option<bool> {
        self.links.claims(id)
    }

    fn next_created_addresses(&mut self, count: usize) -> Result<Vec<u32>> {
        self.links.next_created_addresses(count)
    }

    fn update_observed(
        &mut self,
        id: u32,
        source: u32,
        target: u32,
        observer: ChangeObserver<'_>,
    ) -> Result<Link> {
        let Some(before) = self.visible(id)? else {
            return self.links.update_observed(id, source, target, observer);
        };
        let after = Link::new(id, source, target);
        if !(self.owns(&before) && source == 0)
            && (!self.can_write(&before) || !self.policy_can_write(&after))
        {
            return Err(Forbidden::new(format!("update {id}")).into());
        }
        self.check_references(Some(&before), source, target)?;
        let mut log = LinksUndoLog::new();
        keep_names_before_update(&mut log, &mut self.links, id, source, target)?;
        self.checked(
            &format!("update {id}"),
            id,
            log,
            |links, recorder| links.update_observed(id, source, target, recorder),
            observer,
        )
    }

    fn delete_observed(&mut self, id: u32, observer: ChangeObserver<'_>) -> Result<Link> {
        let Some(before) = self.visible(id)? else {
            return self.links.delete_observed(id, observer);
        };
        if !self.can_write(&before) {
            return Err(Forbidden::new(format!("delete {id}")).into());
        }
        let mut log = LinksUndoLog::new();
        log.keep_names_of_usages(&mut self.links, id)?;
        self.checked(
            &format!("delete {id}"),
            id,
            log,
            |links, recorder| links.delete_observed(id, recorder),
            observer,
        )
    }

    fn all_links(&mut self) -> Vec<Link> {
        let links = self.links.all_links();
        links
            .into_iter()
            .filter(|link| self.can_read(link))
            .collect()
    }

    fn query_links(
        &mut self,
        index: Option<u32>,
        source: Option<u32>,
        target: Option<u32>,
    ) -> Vec<Link> {
        let links = self.links.query_links(index, source, target);
        links
            .into_iter()
            .filter(|link| self.can_read(link))
            .collect()
    }

    fn search(&mut self, source: u32, target: u32) -> Option<u32> {
        let found = self.links.search(source, target)?;
        self.exists(found).then_some(found)
    }

    /// Panics when the subject may not create; use
    /// [`try_get_or_create`](NamedTypeLinks::try_get_or_create).
    fn get_or_create(&mut self, source: u32, target: u32) -> u32 {
        Self::panic_on(self.try_get_or_create(source, target))
    }

    /// The name of a link the subject may refer to; any other has none.
    fn get_name(&mut self, id: u32) -> Result<Option<String>> {
        if !self.can_reference(id) {
            return Ok(None);
        }
        self.links.get_name(id)
    }

    fn set_name(&mut self, id: u32, name: &str) -> Result<u32> {
        self.require_writable(id, "set-name")?;
        if let Some(holder) = self.links.get_by_name(name)? {
            if holder != 0 && holder != id {
                if let Some(link) = self.links.get_link(holder) {
                    if !self.can_write(&link) {
                        // Taking the name takes it from its link.
                        return Err(Forbidden::new(format!("set-name {id}")).into());
                    }
                }
            }
        }
        self.links.set_name(id, name)
    }

    /// The link of a name, when the subject may refer to it; any other name
    /// is an unknown name.
    fn get_by_name(&mut self, name: &str) -> Result<Option<u32>> {
        let found = self.links.get_by_name(name)?;
        Ok(found.filter(|&id| id == 0 || self.can_reference(id)))
    }

    fn remove_name(&mut self, id: u32) -> Result<()> {
        if let Some(link) = self.links.get_link(id) {
            if !self.can_read(&link) {
                // Nothing the subject sees has a name to remove.
                return Ok(());
            }
            self.require_writable(id, "remove-name")?;
        }
        self.links.remove_name(id)
    }

    fn save(&mut self) -> Result<()> {
        self.links.save()
    }
}
