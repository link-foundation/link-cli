//! Any [`NamedTypeLinks`] layer as a [`LinksStorage`], so the generic
//! transactions layer stacks over it (issue #149): a
//! [`PermissionsDecorator`](crate::PermissionsDecorator) of one subject, a
//! [`UnionLinks`](crate::UnionLinks) of a client and a server, a
//! [`RemapLinks`](crate::RemapLinks). Transactions over a permissions view
//! log only the writes the subject may make, as the C# `TransactionsDecorator`
//! over a `PermissionsDecorator` does.
//!
//! The reads of a layer may change it (a permissions view asks its policy, a
//! remap names new addresses), so the layer is kept in a [`RefCell`] and
//! [`LinksStorage`] reads, which take `&self`, borrow it for the call. The
//! layer keeps no links resident of its own to lend, so this is no
//! [`LinksStorageRef`](crate::LinksStorageRef).

use std::cell::RefCell;

use crate::error::LinkError;
use crate::link::Link;
use crate::named_type_links::NamedTypeLinks;
use crate::storage::traits::LinksStorage;

/// A [`NamedTypeLinks`] layer seen as a [`LinksStorage`]; see the
/// [module documentation](self).
pub struct NamedLinksStorage<L: NamedTypeLinks> {
    links: RefCell<L>,
}

impl<L: NamedTypeLinks> NamedLinksStorage<L> {
    /// `links` as a storage.
    pub fn new(links: L) -> Self {
        Self {
            links: RefCell::new(links),
        }
    }

    /// The layer.
    pub fn links_mut(&mut self) -> &mut L {
        self.links.get_mut()
    }

    /// The layer, given back.
    pub fn into_inner(self) -> L {
        self.links.into_inner()
    }
}

/// A [`LinkError`] of a layer's error: the error itself when it is one, a
/// [`LinkError::StorageError`] reading as the error otherwise, so a
/// refused write still reads `Forbidden: ...`.
fn link_error(error: anyhow::Error) -> LinkError {
    match error.downcast::<LinkError>() {
        Ok(error) => error,
        Err(error) => LinkError::StorageError(format!("{error:#}")),
    }
}

impl<L: NamedTypeLinks> LinksStorage<u32> for NamedLinksStorage<L> {
    fn create_link(&mut self, source: u32, target: u32) -> Result<u32, LinkError> {
        self.links
            .get_mut()
            .try_create(source, target)
            .map_err(link_error)
    }

    fn ensure_link_created(&mut self, index: u32) -> Result<u32, LinkError> {
        self.links
            .get_mut()
            .try_ensure_created(index)
            .map_err(link_error)
    }

    fn get_link(&self, index: u32) -> Option<Link> {
        self.links.borrow_mut().get_link(index)
    }

    fn link_exists(&self, index: u32) -> bool {
        self.links.borrow_mut().exists(index)
    }

    fn update_link(&mut self, index: u32, source: u32, target: u32) -> Result<Link, LinkError> {
        self.links
            .get_mut()
            .update(index, source, target)
            .map_err(link_error)
    }

    fn delete_link(&mut self, index: u32) -> Result<Link, LinkError> {
        self.links.get_mut().delete(index).map_err(link_error)
    }

    fn update_link_observed(
        &mut self,
        index: u32,
        source: u32,
        target: u32,
        observer: &mut dyn FnMut(Link, Link),
    ) -> Result<Link, LinkError> {
        self.links
            .get_mut()
            .update_observed(index, source, target, observer)
            .map_err(link_error)
    }

    fn delete_link_observed(
        &mut self,
        index: u32,
        observer: &mut dyn FnMut(Link, Link),
    ) -> Result<Link, LinkError> {
        self.links
            .get_mut()
            .delete_observed(index, observer)
            .map_err(link_error)
    }

    fn all_links(&self) -> Vec<Link> {
        self.links.borrow_mut().all_links()
    }

    fn query_links(
        &self,
        index: Option<u32>,
        source: Option<u32>,
        target: Option<u32>,
    ) -> Vec<Link> {
        self.links.borrow_mut().query_links(index, source, target)
    }

    fn search_link(&self, source: u32, target: u32) -> Option<u32> {
        self.links.borrow_mut().search(source, target)
    }

    fn get_or_create_link(&mut self, source: u32, target: u32) -> Result<u32, LinkError> {
        self.links
            .get_mut()
            .try_get_or_create(source, target)
            .map_err(link_error)
    }

    fn flush(&mut self) -> Result<(), LinkError> {
        self.links.get_mut().save().map_err(link_error)
    }
}
