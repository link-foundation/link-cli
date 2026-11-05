//! [`RemoteLinks`] as a [`Triplets`] store, behind a
//! [`LinksServer`](super::LinksServer) in triplet mode (issue #152).
//!
//! Code written against [`Triplets`], [`process_triplets_query`](crate::process_triplets_query)
//! included, switches from a local store to a server by swapping one value.
//! Every call is one [`TripletsOperation`] round trip. The doublets that store
//! the triplets stay behind the server: over a server in triplet mode, the
//! doublet calls of [`RemoteLinks`] (`create`, `update`, `delete`, `get_link`,
//! `all_links`, `search`) are refused or panic like a failed connection; only
//! `exists`, which counts, and the names keep working.

use super::error::{ProtocolError, ProtocolResult};
use super::links_operations::{parse_count, parse_link_reply, parse_subscription, Part};
use super::remote_links::{connected, RemoteLinks};
use super::triplets_operations::{
    parse_triplet_changes, parse_triplet_notification, parse_triplets, restriction_of_query,
    TripletsNotification, TripletsOperation,
};
use super::LinksOperation;
use crate::triplets::{
    DoubletRole, Triplet, TripletChange, TripletMarkers, TripletQuery, Triplets,
    NOTHING_MARKER_NAME, PAIR_MARKER_NAME,
};
use links_notation::binary::{parse_document, LinoDocument};
use std::time::Duration;

impl RemoteLinks {
    /// Runs one triplet operation on the server and returns its reply.
    pub fn execute_triplets(&self, operation: &TripletsOperation) -> ProtocolResult<LinoDocument> {
        self.client().request(&operation.to_document())
    }

    /// Number of triplets matching `restriction`.
    pub fn count_triplets_matching(&self, restriction: &[Part]) -> ProtocolResult<u32> {
        parse_count(&self.execute_triplets(&TripletsOperation::Count(restriction.to_vec()))?)
    }

    /// Triplets matching `restriction`, ordered by index.
    pub fn triplets_matching(&self, restriction: &[Part]) -> ProtocolResult<Vec<Triplet>> {
        parse_triplets(&self.execute_triplets(&TripletsOperation::Each(restriction.to_vec()))?)
    }

    /// Runs a `create`, `update` or `delete` and returns its changes.
    pub fn triplet_changes(
        &self,
        operation: &TripletsOperation,
    ) -> ProtocolResult<Vec<TripletChange>> {
        parse_triplet_changes(&self.execute_triplets(operation)?)
    }

    /// Asks the server to push the changes other connections make to the
    /// triplets matching `restriction`; returns the subscription number that
    /// [`receive_triplet_notification`](Self::receive_triplet_notification)
    /// reports them with. [`unsubscribe`](Self::unsubscribe) stops it.
    pub fn subscribe_triplets(&self, restriction: &[Part]) -> ProtocolResult<u32> {
        parse_subscription(
            &self.execute_triplets(&TripletsOperation::Subscribe(restriction.to_vec()))?,
        )
    }

    /// The next change pushed to a subscription of this connection, waiting
    /// up to `timeout` for one; `Ok(None)` when none arrived in time.
    pub fn receive_triplet_notification(
        &self,
        timeout: Duration,
    ) -> ProtocolResult<Option<TripletsNotification>> {
        self.client()
            .receive_notification(timeout)?
            .map(|document| parse_triplet_notification(&document))
            .transpose()
    }

    fn marker(&self, name: &str) -> u32 {
        let reply = self.execute(&LinksOperation::GetByName(name.to_string()));
        connected(reply.and_then(|document| parse_link_reply(&document))).unwrap_or(0)
    }

    fn created_triplet(&self, operation: &TripletsOperation) -> ProtocolResult<u32> {
        self.triplet_changes(operation)?
            .into_iter()
            .find_map(|change| change.after.map(|triplet| triplet.index))
            .ok_or_else(|| ProtocolError::malformed("the create reply holds no created triplet"))
    }
}

impl Triplets for RemoteLinks {
    /// The markers the server names; one the subject may not read is `0`.
    /// Nothing else of this implementation depends on them: the server applies
    /// the encoding.
    fn triplet_markers(&self) -> TripletMarkers {
        *self.triplet_markers.get_or_init(|| TripletMarkers {
            pair: self.marker(PAIR_MARKER_NAME),
            nothing: self.marker(NOTHING_MARKER_NAME),
        })
    }

    /// A marker, a triplet, or `None`: the doublets that store the triplets
    /// are not on the wire, so they read as missing.
    fn role_of(&mut self, id: u32) -> Option<DoubletRole> {
        if id == 0 {
            return None;
        }
        if self.triplet_markers().contains(id) {
            return Some(DoubletRole::Marker);
        }
        connected(self.triplets_matching(&[Some(id)]))
            .pop()
            .map(DoubletRole::Triplet)
    }

    fn create_triplet(&mut self, type_id: u32, from: u32, to: u32) -> anyhow::Result<u32> {
        Ok(self.created_triplet(&TripletsOperation::Create { type_id, from, to })?)
    }

    fn create_triplet_point(&mut self) -> anyhow::Result<u32> {
        Ok(self.created_triplet(&TripletsOperation::Create {
            type_id: 0,
            from: 0,
            to: 0,
        })?)
    }

    /// `create` has no way to name an address, a substitution query does.
    fn create_triplet_point_at(&mut self, address: u32) -> anyhow::Result<()> {
        let query = parse_document(&format!("() (({address}: {address} 0 0))"))
            .map_err(|error| anyhow::anyhow!("{error}"))?;
        self.client().request(&query)?;
        Ok(())
    }

    fn update_triplet(
        &mut self,
        index: u32,
        type_id: u32,
        from: u32,
        to: u32,
    ) -> anyhow::Result<Triplet> {
        self.triplet_changes(&TripletsOperation::Update {
            index,
            type_id,
            from,
            to,
        })?
        .into_iter()
        .find_map(|change| change.before)
        .ok_or_else(|| anyhow::anyhow!("the reply holds no change of {index}"))
    }

    fn delete_triplet(&mut self, index: u32) -> anyhow::Result<Vec<Triplet>> {
        Ok(self
            .triplet_changes(&TripletsOperation::Delete(index))?
            .into_iter()
            .filter_map(|change| change.before)
            .collect())
    }

    fn each_triplet(&mut self, query: TripletQuery) -> Vec<Triplet> {
        connected(self.triplets_matching(&restriction_of_query(query)))
    }

    fn count_triplets(&mut self, query: TripletQuery) -> usize {
        connected(self.count_triplets_matching(&restriction_of_query(query))) as usize
    }

    /// How many other triplets use `id`: the doublets behind them are not on
    /// the wire.
    fn doublet_usages(&mut self, id: u32) -> usize {
        let mut users = self.each_triplet(TripletQuery::by_type(id));
        users.extend(self.each_triplet(TripletQuery::by_from(id)));
        users.extend(self.each_triplet(TripletQuery::by_to(id)));
        users.retain(|user| user.index != id);
        users.sort_unstable();
        users.dedup();
        users.len()
    }
}
