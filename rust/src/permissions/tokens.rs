use anyhow::{bail, Result};
use sha2::{Digest, Sha256};

use crate::named_type_links::NamedTypeLinks;
use crate::types::ensure_point;

/// The tokens a links server knows, stored as links (issue #149): the
/// authenticator of `(auth: token)` that `clink --tokens` uses.
///
/// A subject `s` of the data has a pair `(p: Token s)`, where `Token` is the
/// point named `Token`, and each of its tokens is a link `(t: p t)`, named
/// `sha256:<hex>` after the SHA-256 of the token, so the store never holds a
/// token itself. The table lives in a store of its own or in the data; any
/// other authenticator is a callback of
/// [`LinksSessions`](crate::protocol::LinksSessions).
pub struct LinksTokenTable<L: NamedTypeLinks> {
    store: L,
}

impl<L: NamedTypeLinks> LinksTokenTable<L> {
    /// The name of the point every token link starts from.
    pub const TOKEN_NAME: &'static str = "Token";

    pub fn new(store: L) -> Self {
        Self { store }
    }

    pub fn store(&self) -> &L {
        &self.store
    }

    pub fn store_mut(&mut self) -> &mut L {
        &mut self.store
    }

    pub fn into_inner(self) -> L {
        self.store
    }

    /// The name of the link of `token`.
    pub fn name_of(token: &str) -> String {
        let digest = Sha256::digest(token.as_bytes());
        let mut name = String::with_capacity(7 + digest.len() * 2);
        name.push_str("sha256:");
        for byte in digest {
            name.push_str(&format!("{byte:02x}"));
        }
        name
    }

    /// The subject of `token`, or `None` when the table does not know it.
    pub fn authenticate(&mut self, token: &str) -> Result<Option<u32>> {
        let store = &mut self.store;
        let point = store.get_by_name(Self::TOKEN_NAME)?.unwrap_or(0);
        let link = store.get_by_name(&Self::name_of(token))?.unwrap_or(0);
        if point == 0 || link == 0 {
            return Ok(None);
        }
        let Some(stored) = store.get_link(link) else {
            return Ok(None);
        };
        if stored.target != link {
            return Ok(None);
        }
        let pair = stored.source;
        if pair == 0 || pair == link {
            return Ok(None);
        }
        match store.get_link(pair) {
            Some(pair) if pair.source == point && pair.target != 0 => Ok(Some(pair.target)),
            _ => Ok(None),
        }
    }

    /// Lets `token` authenticate as `subject`, instead of any subject it had.
    ///
    /// Returns the link of the token.
    pub fn grant(&mut self, token: &str, subject: u32) -> Result<u32> {
        if subject == 0 {
            bail!("A token needs a subject");
        }
        let store = &mut self.store;
        let point = ensure_point(store, Self::TOKEN_NAME)?;
        let pair = store.try_get_or_create(point, subject)?;
        let name = Self::name_of(token);
        let link = match store.get_by_name(&name)? {
            Some(link) if link != 0 && store.exists(link) => link,
            _ => store.try_create(0, 0)?,
        };
        store.update(link, pair, link)?;
        store.set_name(link, &name)?;
        Ok(link)
    }

    /// Forgets `token`; whether the table knew it.
    pub fn revoke(&mut self, token: &str) -> Result<bool> {
        if self.authenticate(token)?.is_none() {
            return Ok(false);
        }
        let Some(link) = self.store.get_by_name(&Self::name_of(token))? else {
            return Ok(false);
        };
        self.store.remove_name(link)?;
        self.store.delete(link)?;
        Ok(true)
    }
}
