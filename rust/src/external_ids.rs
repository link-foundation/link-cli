//! The ids other systems give our links, stored as links (issue #149): a UUID
//! of a service, an ObjectId of a document database, a URL, bound to a link of
//! ours and back, so the code that talks to the other system never keeps a
//! table of its own.
//!
//! The ids of one kind share a scheme, such as `uuid`. The point named
//! `ExternalId` is `E`, the scheme is the link `(S: E S)` named after it, and
//! a link `l` bound to the id `x` has a pair `(p: S l)` named `scheme:x`, so a
//! link has one id of a scheme and an id one link. No link of the table but a
//! pair starts at `S`, so a pair is never a link of the table itself, whatever
//! address `l` has. The table lives in the data or in a store of its own, as
//! the rules of permissions do; in a store of its own every address it holds
//! but its own is an address of the data.
//!
//! The C# twin is `LinksExternalIds`; in JavaScript a `RemapLinks` reads the
//! links under such ids directly.

use anyhow::{bail, Result};

use crate::named_type_links::NamedTypeLinks;
use crate::types::ensure_point;

/// The name of the point every scheme starts from.
pub const EXTERNAL_ID_NAME: &str = "ExternalId";

/// The ids of one scheme; see the [module documentation](self). Every method
/// takes the store of the table: the data or a store of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinksExternalIds {
    scheme: String,
}

impl LinksExternalIds {
    /// The ids of `scheme`, such as `uuid`; a scheme has no colon.
    pub fn new(scheme: &str) -> Result<Self> {
        if scheme.is_empty() || scheme.contains(':') {
            bail!("A scheme is not empty and has no colon: {scheme:?}");
        }
        Ok(Self {
            scheme: scheme.to_string(),
        })
    }

    /// The kind of the ids.
    pub fn scheme(&self) -> &str {
        &self.scheme
    }

    /// The name of the pair of `id`.
    pub fn name_of(&self, id: &str) -> Result<String> {
        if id.is_empty() {
            bail!("An id is not empty.");
        }
        Ok(format!("{}:{id}", self.scheme))
    }

    /// The link `id` is bound to, `None` when it is bound to none.
    pub fn find(&self, store: &mut dyn NamedTypeLinks, id: &str) -> Result<Option<u32>> {
        Ok(self
            .pair_of(store, id)?
            .and_then(|pair| store.get_link(pair))
            .map(|pair| pair.target))
    }

    /// The id `link` is bound to, `None` when it has none.
    pub fn id_of(&self, store: &mut dyn NamedTypeLinks, link: u32) -> Result<Option<String>> {
        let Some(point) = self.scheme_link(store)? else {
            return Ok(None);
        };
        if link == 0 {
            return Ok(None);
        }
        let Some(pair) = store.search(point, link) else {
            return Ok(None);
        };
        let prefix = format!("{}:", self.scheme);
        Ok(store
            .get_name(pair)?
            .and_then(|name| name.strip_prefix(&prefix).map(str::to_string)))
    }

    /// Binds `id` to `link`, instead of any link it had and of any id the
    /// link had; returns the pair that binds them.
    pub fn bind(&self, store: &mut dyn NamedTypeLinks, id: &str, link: u32) -> Result<u32> {
        let name = self.name_of(id)?;
        if link == 0 {
            bail!("The null link has no id.");
        }
        self.unbind(store, id)?;
        let point = match self.scheme_link(store)? {
            Some(point) => point,
            None => {
                let root = ensure_point(store, EXTERNAL_ID_NAME)?;
                let point = store.try_create(0, 0)?;
                store.update(point, root, point)?;
                store.set_name(point, &self.scheme)?;
                point
            }
        };
        let pair = match store.search(point, link) {
            Some(pair) => pair,
            None => store.try_create(point, link)?,
        };
        store.set_name(pair, &name)?;
        Ok(pair)
    }

    /// The link `id` is bound to, a new point `(l: l l)` of `data` bound to
    /// it when it has none. `data` is `None` when the table is in the data.
    pub fn ensure(
        &self,
        store: &mut dyn NamedTypeLinks,
        data: Option<&mut dyn NamedTypeLinks>,
        id: &str,
    ) -> Result<u32> {
        if let Some(found) = self.find(store, id)? {
            return Ok(found);
        }
        let link = match data {
            Some(data) => new_point(data)?,
            None => new_point(store)?,
        };
        self.bind(store, id, link)?;
        Ok(link)
    }

    /// Unbinds `id`; whether it was bound.
    pub fn unbind(&self, store: &mut dyn NamedTypeLinks, id: &str) -> Result<bool> {
        let Some(pair) = self.pair_of(store, id)? else {
            return Ok(false);
        };
        store.remove_name(pair)?;
        store.delete(pair)?;
        Ok(true)
    }

    fn pair_of(&self, store: &mut dyn NamedTypeLinks, id: &str) -> Result<Option<u32>> {
        let name = self.name_of(id)?;
        let Some(point) = self.scheme_link(store)? else {
            return Ok(None);
        };
        let Some(pair) = store.get_by_name(&name)?.filter(|&pair| pair != 0) else {
            return Ok(None);
        };
        Ok(store
            .get_link(pair)
            .filter(|link| link.source == point)
            .map(|_| pair))
    }

    /// The link `(S: E S)` of the scheme, `None` when the table has none yet.
    fn scheme_link(&self, store: &mut dyn NamedTypeLinks) -> Result<Option<u32>> {
        let Some(root) = store
            .get_by_name(EXTERNAL_ID_NAME)?
            .filter(|&root| root != 0)
        else {
            return Ok(None);
        };
        let Some(point) = store.get_by_name(&self.scheme)?.filter(|&point| point != 0) else {
            return Ok(None);
        };
        Ok(store
            .get_link(point)
            .filter(|link| link.source == root && link.target == point)
            .map(|_| point))
    }
}

fn new_point(links: &mut dyn NamedTypeLinks) -> Result<u32> {
    let link = links.try_create(0, 0)?;
    links.update(link, link, link)?;
    Ok(link)
}
