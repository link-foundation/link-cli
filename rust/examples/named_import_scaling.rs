//! Finite synthetic fixture for issue #110. Run in release mode.
use anyhow::{ensure, Result};
use link_cli::{import_lino_text, NamedTypeLinks, NamedTypesDecorator, Parser};
use std::time::Instant;
use tempfile::tempdir;

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("named");
    let count: usize = args.get(2).map(|s| s.parse()).transpose()?.unwrap_or(100);
    ensure!(count <= 10_000, "fixture is limited to 10,000 links");
    let text = (1..=count)
        .map(|i| {
            let id = if mode == "numeric" {
                i.to_string()
            } else {
                format!("atom-{i:064}")
            };
            format!("({id}: {id} {id})\n")
        })
        .collect::<String>();
    let start = Instant::now();
    let parsed = Parser::new().parse(&text)?;
    let parser_ms = start.elapsed().as_secs_f64() * 1000.0;
    ensure!(parsed.len() == count, "unexpected parser output");
    drop(parsed);
    let dir = tempdir()?;
    let mut storage = NamedTypesDecorator::new(dir.path().join("data.links"), false)?;
    if args.get(3).is_some_and(|arg| arg == "indexed") {
        storage = storage.with_indexed_lookups();
    }
    let start = Instant::now();
    import_lino_text(&mut storage, &text)?;
    let import_ms = start.elapsed().as_secs_f64() * 1000.0;
    let start = Instant::now();
    NamedTypeLinks::save(&mut storage)?;
    let save_ms = start.elapsed().as_secs_f64() * 1000.0;
    println!("mode,count,bytes,parser_ms,import_including_parser_ms,save_ms");
    println!(
        "{mode},{count},{},{parser_ms:.3},{import_ms:.3},{save_ms:.3}",
        text.len()
    );
    Ok(())
}
