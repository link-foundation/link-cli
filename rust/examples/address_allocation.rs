//! Selected coordinator: endpoint-list prefix/ author token count.
//! Keep the exact request to resolve uncertain replies after reconnecting.
use link_cli::engine::{
    address_allocator::{AddressAllocator, AddressAllocatorOptions},
    etcd::EtcdJournalStore,
};
fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [endpoints, prefix, author, token, count] = args.as_slice() else {
        anyhow::bail!("Use address_allocation endpoints prefix/ author token count");
    };
    let store = EtcdJournalStore::new(
        endpoints.split(',').map(String::from).collect(),
        prefix.clone(),
    )?;
    let result = AddressAllocator::new(
        store,
        "addresses",
        1,
        u64::MAX,
        AddressAllocatorOptions {
            require_durable_flush: true,
            ..Default::default()
        },
    )?
    .reserve(author, token, count.parse()?)?;
    println!(
        "{}",
        serde_json::json!({"author":result.author,"token":result.token,"count":result.count.to_string(),"first":result.first.to_string(),"last":result.last.to_string()})
    );
    Ok(())
}
