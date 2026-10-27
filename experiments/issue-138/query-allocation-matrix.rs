//! Finite comparison of reference planning with recycling and monotonic stores.
//! Compile against an existing non-instrumented link_cli rlib; no source edits.
use link_cli::engine::{Engine, EngineOptions, MemoryLog};
use link_cli::{LinkStorage, LinksStorage, NamedTypeLinks, QueryProcessor};

fn main() {
    let directory = std::env::temp_dir().join(format!("link-query-allocation-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let cases = [
        (false, "(() ((1 1)))"),
        (false, "(() ((1 2) (2 1)))"),
        (false, "(() ((1: 1 1) (2 2)))"),
        (false, "(() ((100: 100 100) (1 1)))"),
        (false, "(() ((foo: foo foo)))"),
        (false, "(() ((foo: (1 1) 1)))"),
        (false, "(() ((foo: foo foo) (2 2)))"),
        (false, "(() ((1 1) (2: 2 2) (3 3)))"),
        (true, "(() ((1 1)))"),
        (true, "(() ((1 2) (2 1)))"),
        (true, "(() ((1: 1 1) (2 2)))"),
        (true, "(() ((100 100)))"),
        (true, "(() ((foo: (1 1) 1)))"),
        (true, "(() ((foo: foo foo) (2 2)))"),
        (true, "(() ((1 1) (2: 2 2) (3 3)))"),
    ];
    for (index, (auto, query)) in cases.iter().enumerate() {
        let engine = Engine::<u32, _>::open(MemoryLog::default(), EngineOptions::default()).unwrap();
        let mut tx = engine.begin().unwrap();
        let processor = QueryProcessor::new(false).with_auto_create_missing_references(*auto);
        let mut recycling = LinkStorage::new(directory.join(format!("{index}.links")), false).unwrap();
        println!("CASE {index}: auto={auto}, {query}");
        for (name, result, rows) in [
            ("monotonic", processor.process_query(&mut tx, query), LinksStorage::all_links(&tx)),
            ("recycling", processor.process_query(&mut recycling, query), NamedTypeLinks::all_links(&mut recycling)),
        ] {
            let missing: Vec<_> = rows.iter().flat_map(|link| [link.source, link.target])
                .filter(|part| *part != 0 && !rows.iter().any(|link| link.index == *part)).collect();
            println!("  {name}: result={:?}; rows={rows:?}; missing={missing:?}", result.map(|changes| changes.len()).map_err(|error| error.to_string()));
        }
        assert!(engine.snapshot().unwrap().links().is_empty());
    }
    std::fs::remove_dir_all(directory).unwrap();
    println!("COMPLETE 15 finite cases; each engine snapshot remains unpublished.");
}
