//! Small public pieces no larger test goes through on its way: accessors, debug
//! text, and answers a token table gives about links that are no longer as it wrote.

use anyhow::Result;
use link_cli::protocol::{LinksServer, ServerOptions};
use link_cli::sequences::DefaultStack;
use link_cli::{LinkStorage, LinksTokenTable, NamedTypeLinks};

#[test]
fn a_stack_knows_its_length() {
    let mut stack = DefaultStack::new();
    assert!(stack.is_empty());
    stack.push(7);
    assert_eq!(stack.len(), 1);
    assert!(!stack.is_empty());
    assert_eq!(stack.pop(), Some(7));
    assert_eq!(stack.pop(), None);
}

#[test]
fn a_server_describes_itself_without_its_store() -> Result<()> {
    let server = LinksServer::bind("127.0.0.1:0", ServerOptions::default())?;
    let text = format!("{server:?}");
    assert!(text.starts_with("LinksServer"), "{text}");
    assert!(text.contains("readable: false"), "{text}");
    Ok(())
}

fn table_with_a_token() -> Result<(LinksTokenTable<LinkStorage>, u32, u32)> {
    let mut table = LinksTokenTable::new(LinkStorage::in_memory(false));
    let subject = table.store_mut().create(0, 0);
    let link = table.grant("secret", subject)?;
    assert_eq!(table.authenticate("secret")?, Some(subject));
    Ok((table, subject, link))
}

#[test]
fn a_token_needs_a_subject() -> Result<()> {
    let (mut table, _, _) = table_with_a_token()?;
    assert!(table.grant("other", 0).is_err());
    assert_eq!(table.authenticate("other")?, None);
    Ok(())
}

#[test]
fn a_table_gives_back_its_store() -> Result<()> {
    let (table, subject, link) = table_with_a_token()?;
    assert!(table.store().all().iter().any(|row| row.index == link));
    let mut store = table.into_inner();
    assert!(store.exists(subject));
    Ok(())
}

#[test]
fn a_token_whose_link_was_changed_authenticates_nobody() -> Result<()> {
    // No longer the link of a token: it does not point at itself.
    let (mut table, subject, link) = table_with_a_token()?;
    table.store_mut().update(link, subject, subject)?;
    assert_eq!(table.authenticate("secret")?, None);
    assert!(!table.revoke("secret")?);

    // No pair at all, or itself where the pair should be.
    let (mut table, _, link) = table_with_a_token()?;
    table.store_mut().update(link, 0, link)?;
    assert_eq!(table.authenticate("secret")?, None);
    let (mut table, _, link) = table_with_a_token()?;
    table.store_mut().update(link, link, link)?;
    assert_eq!(table.authenticate("secret")?, None);

    // A pair that is not of the token point.
    let (mut table, subject, link) = table_with_a_token()?;
    let other = table.store_mut().create(subject, subject);
    table.store_mut().update(link, other, link)?;
    assert_eq!(table.authenticate("secret")?, None);

    // A pair that is gone.
    let (mut table, _, link) = table_with_a_token()?;
    let pair = table.store_mut().get_link(link).unwrap().source;
    table.store_mut().update(link, 0, link)?;
    table.store_mut().delete(pair)?;
    table.store_mut().update(link, pair, link)?;
    assert_eq!(table.authenticate("secret")?, None);
    Ok(())
}
