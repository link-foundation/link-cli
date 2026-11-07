//! Durable, retryable attribution of one journal decision to its branch.
use super::{
    decode_version_name, encode_version_name, GenericVersionControlDecorator, NamedTypeLinks,
    Result, VersionControlTransactionState,
};
use anyhow::bail;

pub(super) const PUBLICATION_PREFIX: &str = "__vc:pending:";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PublicationIntent {
    pub id: u128,
    pub before: i64,
    pub after: i64,
    pub branch: String,
}

impl PublicationIntent {
    pub fn encode(&self) -> String {
        format!(
            "{PUBLICATION_PREFIX}2|{:032x}|{}|{}|{}",
            self.id,
            self.before,
            self.after,
            encode_version_name(&self.branch)
        )
    }

    pub fn decode(marker: &str) -> Option<Self> {
        let fields: Vec<_> = marker
            .strip_prefix(PUBLICATION_PREFIX)?
            .split('|')
            .collect();
        if fields.len() != 5 || fields[0] != "2" || fields[1].len() != 32 {
            return None;
        }
        let id = u128::from_str_radix(fields[1], 16).ok()?;
        if id == 0 || !fields[1].bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        if fields[2..=3]
            .iter()
            .any(|field| field.is_empty() || !field.bytes().all(|byte| byte.is_ascii_digit()))
        {
            return None;
        }
        let before = fields[2].parse::<i64>().ok()?;
        let after = fields[3].parse::<i64>().ok()?;
        let branch = decode_version_name(fields[4])?;
        if after < before || branch.trim().is_empty() {
            return None;
        }
        Some(Self {
            id,
            before,
            after,
            branch,
        })
    }
}

impl<B: NamedTypeLinks> GenericVersionControlDecorator<B> {
    pub(super) fn prepare_publication(&mut self) -> Result<VersionControlTransactionState> {
        let state = self
            .active_transaction
            .as_mut()
            .expect("version transaction is open");
        // Freeze and fence before any metadata operation can fail.
        let after = *state
            .after_sequence
            .get_or_insert(self.transactions.last_logged_sequence());
        let intent = PublicationIntent {
            id: state.id,
            before: state.before_sequence,
            after,
            branch: state.branch_name.clone(),
        };
        let marker = intent.encode();
        if state.intent_link == 0 || !self.branches_store.exists(state.intent_link) {
            state.intent_link = match self.branches_store.get_by_name(&marker)? {
                Some(link) => link,
                None => self.branches_store.try_create(0, 0)?,
            };
        }
        if self.branches_store.get_name(state.intent_link)?.as_deref() != Some(marker.as_str()) {
            self.branches_store.set_name(state.intent_link, &marker)?;
        }
        // Persist topology/current selection along with the intent, before the
        // journal decision can require its attribution after an abrupt exit.
        self.branches_store.save()?;
        let state = state.clone();
        self.trace(&format!(
            "Prepared version publication {:032x}: {}..{} on '{}'.",
            intent.id, intent.before, intent.after, intent.branch
        ));
        Ok(state)
    }

    pub(super) fn clear_publication_intent(&mut self, link: u32) -> Result<()> {
        if link != 0 {
            self.branches_store.remove_name(link)?;
            if self.branches_store.exists(link) {
                self.branches_store.delete(link)?;
            }
        }
        // Even a removal that took effect before returning an error must have
        // its final flush retried before admission is released.
        self.branches_store.save()
    }

    pub(super) fn finish_aborted_publication(
        &mut self,
        state: &VersionControlTransactionState,
    ) -> Result<()> {
        if state.after_sequence.is_some() {
            self.clear_publication_intent(state.intent_link)?;
        }
        self.active_transaction = None;
        Ok(())
    }

    pub(super) fn recover_publications(
        &mut self,
        pending: Vec<(u32, PublicationIntent)>,
    ) -> Result<()> {
        if pending.is_empty() {
            return Ok(());
        }
        if pending.len() != 1 {
            bail!("Multiple version publication intents.");
        }
        let (link, intent) = &pending[0];
        let branch = self.branches.get(&intent.branch).ok_or_else(|| {
            anyhow::anyhow!("Version publication intent contradicts branch metadata.")
        })?;
        if self.current_branch != intent.branch
            || branch.head > intent.after
            || self.current_applied > intent.after
        {
            bail!("Version publication intent contradicts branch metadata.");
        }
        let committed = self.transactions.is_transaction_committed(intent.id);
        if committed && self.transactions.is_transaction_rolled_back(intent.id) {
            bail!("Version publication has conflicting transaction decisions.");
        }
        let records: Vec<_> = self
            .transactions
            .log()
            .into_iter()
            .filter(|t| t.sequence > intent.before && t.sequence <= intent.after)
            .collect();
        if records.iter().any(|t| t.transaction_id != intent.id)
            || self.transition_branches.iter().any(|(&seq, branch)| {
                seq > intent.before && seq <= intent.after && branch != &intent.branch
            })
            || (committed && records.len() as i64 != intent.after - intent.before)
        {
            bail!("Version publication intent contradicts its transition range.");
        }
        if committed {
            self.attribute_new_transitions_for_branch(intent.before, intent.after, &intent.branch)?;
            self.branches_store.save()?;
        }
        // The transaction constructor has already durably redone committed
        // records and undone pending records before sidecar recovery starts.
        self.clear_publication_intent(*link)?;
        self.trace(&format!(
            "Recovered {} version publication {:032x}.",
            if committed { "committed" } else { "aborted" },
            intent.id
        ));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_publication_markers_round_trip_identity_range_and_name() {
        let vectors: serde_json::Value = serde_json::from_str(include_str!(
            "../../../docs/protocol/version-publication-vectors.json"
        ))
        .unwrap();
        for row in vectors["intents"].as_array().unwrap() {
            let expected = PublicationIntent {
                id: u128::from_str_radix(row["id"].as_str().unwrap(), 16).unwrap(),
                before: row["before"].as_i64().unwrap(),
                after: row["after"].as_i64().unwrap(),
                branch: row["branch"].as_str().unwrap().into(),
            };
            let marker = row["marker"].as_str().unwrap();
            assert_eq!(PublicationIntent::decode(marker), Some(expected.clone()));
            assert_eq!(expected.encode(), marker);
        }
        for marker in vectors["invalidIntents"].as_array().unwrap() {
            assert!(
                PublicationIntent::decode(marker.as_str().unwrap()).is_none(),
                "{marker}"
            );
        }
    }
}
