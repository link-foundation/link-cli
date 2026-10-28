//! Explicit application-authorized takeover: endpoints prefix/ author predecessor token.
use link_cli::engine::{
    author_fence::{AuthorFence, AuthorFenceOptions},
    etcd::EtcdJournalStore,
};
fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [endpoints, prefix, author, predecessor, token] = args.as_slice() else {
        anyhow::bail!("Use author_fence endpoints prefix/ author predecessor token");
    };
    let store = EtcdJournalStore::new(
        endpoints.split(',').map(String::from).collect(),
        prefix.clone(),
    )?;
    let claim = AuthorFence::new(
        store,
        "fence",
        author,
        AuthorFenceOptions {
            require_durable_flush: true,
            ..Default::default()
        },
    )?
    .claim(predecessor.parse()?, token)?;
    println!(
        "{}",
        serde_json::json!({"author":claim.author,"token":claim.token,"epoch":claim.epoch.to_string()})
    );
    Ok(())
}
