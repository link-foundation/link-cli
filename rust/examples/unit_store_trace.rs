//! Replays a script of link operations against a file-mapped
//! `DoubletsStorage` and prints the result of every operation, so another
//! implementation of the unit store (the JavaScript port in
//! `js/server/src/unit-store*.js`) can be checked byte for byte against the
//! database file this writes.
//!
//! ```text
//! cargo run --manifest-path rust/Cargo.toml --example unit_store_trace -- \
//!     <database> <script> [--resolved] [--u32]
//! ```
//!
//! The script holds one operation per line (`#` starts a comment):
//!
//! ```text
//! create <source> <target>      LinksStorage::create_link
//! point                         Doublets::create_point
//! update <index> <source> <target>
//! delete <index>
//! ensure <index>                LinksStorage::ensure_link_created
//! get-or-create <source> <target>
//! ```
//!
//! `--resolved` wraps the store in
//! `with_automatic_uniqueness_and_usages_resolution`; `--u32` addresses
//! links by `u32` instead of `usize`. A failed operation prints
//! `error <message>` and the replay goes on.

use std::path::Path;

use link_cli::doublets::data::LinkReference;
use link_cli::doublets::Doublets;
use link_cli::storage::LinksStorage;
use link_cli::{DoubletsStorage, LinkError};

fn number<T: LinkReference>(text: Option<&str>) -> Result<T, String> {
    let text = text.ok_or("missing operand")?;
    let value: usize = text.parse().map_err(|_| format!("bad number {text}"))?;
    T::from_usize(value).ok_or_else(|| format!("{value} does not fit the address type"))
}

fn apply<T, S>(storage: &mut DoubletsStorage<T, S>, line: &str) -> Result<String, String>
where
    T: LinkReference,
    S: Doublets<T>,
{
    let mut words = line.split_whitespace();
    let operation = words.next().unwrap_or_default();
    let mut next = || number::<T>(words.next());
    let shown = |result: Result<T, LinkError>| result.map(|value| value.to_string());
    let link = |result: Result<link_cli::link::GenericLink<T>, LinkError>| {
        result.map(|link| format!("{} {} {}", link.index, link.source, link.target))
    };
    let outcome = match operation {
        "create" => {
            let (source, target) = (next()?, next()?);
            shown(storage.create_link(source, target))
        }
        "point" => shown(storage.store_mut().create_point().map_err(LinkError::from)),
        "update" => {
            let (index, source, target) = (next()?, next()?, next()?);
            link(storage.update_link(index, source, target))
        }
        "delete" => link(storage.delete_link(next()?)),
        "ensure" => shown(storage.ensure_link_created(next()?)),
        "get-or-create" => {
            let (source, target) = (next()?, next()?);
            shown(storage.get_or_create_link(source, target))
        }
        other => return Err(format!("unknown operation {other}")),
    };
    Ok(outcome.unwrap_or_else(|error| format!("error {error}")))
}

fn replay<T, S>(mut storage: DoubletsStorage<T, S>, script: &str) -> Result<(), String>
where
    T: LinkReference,
    S: Doublets<T>,
{
    for line in script.lines() {
        let line = line.split('#').next().unwrap_or_default().trim();
        if !line.is_empty() {
            println!("{}", apply(&mut storage, line)?);
        }
    }
    println!("count {}", storage.links_count());
    storage.flush().map_err(|error| error.to_string())
}

fn run<T: LinkReference>(database: &Path, script: &str, resolved: bool) -> Result<(), String> {
    let storage = DoubletsStorage::<T, _>::open(database).map_err(|error| error.to_string())?;
    if resolved {
        replay(
            storage.with_automatic_uniqueness_and_usages_resolution(),
            script,
        )
    } else {
        replay(storage, script)
    }
}

fn main() -> Result<(), String> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| arguments.iter().any(|argument| argument == name);
    let mut paths = arguments
        .iter()
        .filter(|argument| !argument.starts_with("--"));
    let usage = "usage: unit_store_trace <database> <script> [--resolved] [--u32]";
    let database = Path::new(paths.next().ok_or(usage)?);
    let script =
        std::fs::read_to_string(paths.next().ok_or(usage)?).map_err(|error| error.to_string())?;
    if flag("--u32") {
        run::<u32>(database, &script, flag("--resolved"))
    } else {
        run::<usize>(database, &script, flag("--resolved"))
    }
}
