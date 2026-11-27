//! One transaction for the local CLI write pipeline.

use anyhow::Result;
use link_cli::cli::Cli;
use link_cli::{FileTransactionsDecorator, FileVersionControlDecorator, NamedTypeLinks};

pub(super) fn finish<S: CliTransaction>(cli: &Cli, storage: &mut S) -> Result<()> {
    let query = cli.query.as_deref().or(cli.query_arg.as_deref());
    let may_write = cli.lino_input.is_some()
        || cli.binary_input.is_some()
        || query.is_some_and(|text| !text.is_empty());
    if !may_write || cli.serve.is_some() || cli.grant_token.is_some() || cli.revoke_token.is_some()
    {
        return super::finish(cli, storage);
    }

    let before = storage.transactions().last_logged_sequence();
    storage.transactions().defer_batch_checkpoint();
    storage.begin()?;
    if let Err(error) = super::finish(cli, storage) {
        storage.rollback().map_err(|rollback| {
            rollback.context(format!(
                "Failed to roll back CLI transaction after: {error:#}"
            ))
        })?;
        return Err(error);
    }
    if storage.transactions().last_logged_sequence() == before {
        // An unchanged import or a read-only query has no journal decision.
        storage.rollback()
    } else {
        storage.commit()
    }
}

pub(super) trait CliTransaction: NamedTypeLinks {
    fn transactions(&mut self) -> &mut FileTransactionsDecorator;
    fn begin(&mut self) -> Result<()>;
    fn commit(&mut self) -> Result<()>;
    fn rollback(&mut self) -> Result<()>;
}

impl CliTransaction for FileTransactionsDecorator {
    fn transactions(&mut self) -> &mut FileTransactionsDecorator {
        self
    }

    fn begin(&mut self) -> Result<()> {
        self.begin_transaction()?;
        Ok(())
    }

    fn commit(&mut self) -> Result<()> {
        Ok(Self::commit(self)?)
    }

    fn rollback(&mut self) -> Result<()> {
        Ok(Self::rollback(self)?)
    }
}

impl CliTransaction for FileVersionControlDecorator {
    fn transactions(&mut self) -> &mut FileTransactionsDecorator {
        self.transactions_mut()
    }

    fn begin(&mut self) -> Result<()> {
        self.begin_transaction()?;
        Ok(())
    }

    fn commit(&mut self) -> Result<()> {
        Self::commit(self)
    }

    fn rollback(&mut self) -> Result<()> {
        Self::rollback(self)
    }
}
