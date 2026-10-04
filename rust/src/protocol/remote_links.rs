//! [`RemoteLinks`]: a links store that lives behind a [`LinksServer`](super::LinksServer).
//!
//! It implements the same interfaces as a local store — [`doublets::Links`],
//! [`doublets::Doublets`] and [`NamedTypeLinks`] — so code written against
//! them, the [`QueryProcessor`](crate::QueryProcessor) included, switches
//! from a local file to a server by swapping one value:
//!
//! ```no_run
//! use link_cli::protocol::{RemoteLinks, TextLinoProtocol};
//! use link_cli::{NamedTypeLinks, NamedTypesDecorator, QueryProcessor};
//!
//! fn run(store: &mut impl NamedTypeLinks) -> anyhow::Result<()> {
//!     QueryProcessor::new(false).process_query(store, "() ((1 1))")?;
//!     Ok(())
//! }
//!
//! run(&mut NamedTypesDecorator::new("db.links", false)?)?;
//! run(&mut RemoteLinks::connect("127.0.0.1:8080", TextLinoProtocol::new())?)?;
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! Every call is one [`LinksOperation`] round trip. The methods of the
//! upstream and CLI traits that cannot return an error panic when the
//! connection fails, the way a local store panics when its file does; the
//! inherent methods return every failure as a [`ProtocolError`].

use super::client::LinksClient;
use super::error::{ProtocolError, ProtocolResult};
use super::links_operations::{
    parse_changes, parse_count, parse_link_reply, parse_links, parse_name, Change, LinksOperation,
    Part,
};
use super::mapping::LinoDocument;
use super::protocols::LinoProtocol;
use crate::link::Link;
use crate::link_storage::ChangeObserver;
use crate::link_storage_doublets::link_storage_constants;
use crate::named_type_links::NamedTypeLinks;
use doublets::data::{Flow, LinksConstants, ReadHandler, WriteHandler};
use doublets::{Doublets, Error, Link as DoubletsLink, Links};
use std::net::ToSocketAddrs;
use std::sync::{Mutex, PoisonError};

/// A links store served by a [`LinksServer`](super::LinksServer), usable
/// wherever a local store is.
#[derive(Debug)]
pub struct RemoteLinks {
    client: Mutex<LinksClient>,
}

impl RemoteLinks {
    /// Connects to the server at `address`.
    pub fn connect(
        address: impl ToSocketAddrs,
        protocol: impl LinoProtocol + 'static,
    ) -> ProtocolResult<Self> {
        Ok(Self::new(LinksClient::connect(address, protocol)?))
    }

    /// Uses an existing connection.
    pub fn new(client: LinksClient) -> Self {
        Self {
            client: Mutex::new(client),
        }
    }

    /// Runs one operation on the server and returns its reply.
    pub fn execute(&self, operation: &LinksOperation) -> ProtocolResult<LinoDocument> {
        self.client
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .request(&operation.to_document())
    }

    /// Number of links matching `restriction`.
    pub fn count_matching(&self, restriction: &[Part]) -> ProtocolResult<u32> {
        parse_count(&self.execute(&LinksOperation::Count(restriction.to_vec()))?)
    }

    /// Links matching `restriction`, ordered by index.
    pub fn links_matching(&self, restriction: &[Part]) -> ProtocolResult<Vec<Link>> {
        parse_links(&self.execute(&LinksOperation::Each(restriction.to_vec()))?)
    }

    /// Runs a `create`, `update` or `delete` and returns its changes.
    pub fn changes(&self, operation: &LinksOperation) -> ProtocolResult<Vec<Change>> {
        parse_changes(&self.execute(operation)?)
    }

    /// The link stored at `index`.
    pub fn link(&self, index: u32) -> ProtocolResult<Option<Link>> {
        Ok(self.links_matching(&[Some(index)])?.pop())
    }

    fn create_remote(&self, source: u32, target: u32) -> ProtocolResult<Link> {
        self.changes(&LinksOperation::Create { source, target })?
            .into_iter()
            .find_map(|(before, after)| (before.is_null() && !after.is_null()).then_some(after))
            .ok_or_else(|| ProtocolError::malformed("the create reply holds no created link"))
    }

    fn removed_link(changes: &[Change], index: u32) -> ProtocolResult<Link> {
        changes
            .iter()
            .find(|(before, _)| before.index == index)
            .map(|(before, _)| *before)
            .ok_or_else(|| {
                ProtocolError::malformed(format!("the reply holds no change of {index}"))
            })
    }

    fn parts(&self, query: &[u32]) -> Vec<Part> {
        let any = self.constants().any;
        query
            .iter()
            .map(|&part| (part != any).then_some(part))
            .collect()
    }
}

/// Unwraps the result of a call whose interface has no way to report a
/// failure.
fn connected<T>(result: ProtocolResult<T>) -> T {
    result.unwrap_or_else(|error| panic!("remote links store failed: {error}"))
}

fn doublets_link(link: &Link) -> DoubletsLink<u32> {
    DoubletsLink::new(link.index, link.source, link.target)
}

fn doublets_error(error: ProtocolError) -> Error<u32> {
    Error::Other(Box::new(error))
}

/// Feeds `changes` to `handler` until it asks to stop.
fn replay(changes: &[Change], handler: WriteHandler<'_, u32>) -> Flow {
    for (before, after) in changes {
        if handler(doublets_link(before), doublets_link(after)) == Flow::Break {
            return Flow::Break;
        }
    }
    Flow::Continue
}

fn part(query: &[u32], index: usize) -> u32 {
    query.get(index).copied().unwrap_or(0)
}

impl Links<u32> for RemoteLinks {
    fn constants(&self) -> &LinksConstants<u32> {
        link_storage_constants()
    }

    fn count_links(&self, query: &[u32]) -> u32 {
        connected(self.count_matching(&self.parts(query)))
    }

    /// Creates an empty `(index: 0 0)` link, like every `doublets` store.
    fn create_links(
        &mut self,
        _query: &[u32],
        handler: WriteHandler<'_, u32>,
    ) -> Result<Flow, Error<u32>> {
        let changes = self
            .changes(&LinksOperation::Create {
                source: 0,
                target: 0,
            })
            .map_err(doublets_error)?;
        Ok(replay(&changes, handler))
    }

    fn each_links(&self, query: &[u32], handler: ReadHandler<'_, u32>) -> Flow {
        for link in connected(self.links_matching(&self.parts(query))) {
            if handler(doublets_link(&link)) == Flow::Break {
                return Flow::Break;
            }
        }
        Flow::Continue
    }

    fn update_links(
        &mut self,
        query: &[u32],
        change: &[u32],
        handler: WriteHandler<'_, u32>,
    ) -> Result<Flow, Error<u32>> {
        let changes = self
            .changes(&LinksOperation::Update {
                index: part(query, 0),
                source: part(change, 1),
                target: part(change, 2),
            })
            .map_err(doublets_error)?;
        Ok(replay(&changes, handler))
    }

    fn delete_links(
        &mut self,
        query: &[u32],
        handler: WriteHandler<'_, u32>,
    ) -> Result<Flow, Error<u32>> {
        let changes = self
            .changes(&LinksOperation::Delete(part(query, 0)))
            .map_err(doublets_error)?;
        Ok(replay(&changes, handler))
    }
}

impl Doublets<u32> for RemoteLinks {
    fn get_link(&self, index: u32) -> Option<DoubletsLink<u32>> {
        connected(self.link(index)).as_ref().map(doublets_link)
    }
}

impl NamedTypeLinks for RemoteLinks {
    fn create(&mut self, source: u32, target: u32) -> u32 {
        connected(self.create_remote(source, target)).index
    }

    /// Asks for links until the server hands out `id`, then deletes the ones
    /// handed out on the way, exactly like [`LinkStorage::ensure_created`](crate::LinkStorage::ensure_created).
    fn ensure_created(&mut self, id: u32) -> u32 {
        if id == 0 || self.exists(id) {
            return id;
        }
        let mut passed_over = Vec::new();
        loop {
            let created = NamedTypeLinks::create(self, 0, 0);
            if created == id {
                break;
            }
            passed_over.push(created);
        }
        for address in passed_over {
            connected(self.changes(&LinksOperation::Delete(address)));
        }
        id
    }

    fn get_link(&mut self, id: u32) -> Option<Link> {
        connected(self.link(id))
    }

    fn exists(&mut self, id: u32) -> bool {
        connected(self.link(id)).is_some()
    }

    fn update(&mut self, id: u32, source: u32, target: u32) -> anyhow::Result<Link> {
        let changes = self.changes(&LinksOperation::Update {
            index: id,
            source,
            target,
        })?;
        Ok(Self::removed_link(&changes, id)?)
    }

    fn delete(&mut self, id: u32) -> anyhow::Result<Link> {
        self.delete_observed(id, &mut |_, _| {})
    }

    fn delete_observed(&mut self, id: u32, observer: ChangeObserver<'_>) -> anyhow::Result<Link> {
        let changes = self.changes(&LinksOperation::Delete(id))?;
        for (before, after) in &changes {
            observer(*before, *after);
        }
        Ok(Self::removed_link(&changes, id)?)
    }

    fn all_links(&mut self) -> Vec<Link> {
        connected(self.links_matching(&[]))
    }

    fn search(&mut self, source: u32, target: u32) -> Option<u32> {
        connected(self.links_matching(&[None, Some(source), Some(target)]))
            .first()
            .map(|link| link.index)
    }

    fn get_or_create(&mut self, source: u32, target: u32) -> u32 {
        match self.search(source, target) {
            Some(index) => index,
            None => NamedTypeLinks::create(self, source, target),
        }
    }

    fn get_name(&mut self, id: u32) -> anyhow::Result<Option<String>> {
        Ok(parse_name(&self.execute(&LinksOperation::GetName(id))?)?)
    }

    fn set_name(&mut self, id: u32, name: &str) -> anyhow::Result<u32> {
        parse_link_reply(&self.execute(&LinksOperation::SetName(id, name.to_string()))?)?
            .ok_or_else(|| anyhow::anyhow!("the set-name reply holds no link"))
    }

    fn get_by_name(&mut self, name: &str) -> anyhow::Result<Option<u32>> {
        Ok(parse_link_reply(
            &self.execute(&LinksOperation::GetByName(name.to_string()))?,
        )?)
    }

    fn remove_name(&mut self, id: u32) -> anyhow::Result<()> {
        self.execute(&LinksOperation::RemoveName(id))?;
        Ok(())
    }

    /// The server saves after every change, so there is nothing to flush.
    fn save(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
}
