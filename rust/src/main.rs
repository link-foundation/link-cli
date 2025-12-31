//! Link CLI - A command-line tool for links manipulation
//!
//! This is the Rust implementation of the link-cli tool, providing
//! similar functionality to the C# version.

use anyhow::Result;
use clap::Parser;
use link_cli::{LinkStorage, QueryProcessor};

/// Link CLI - A CLI tool for managing links data store
#[derive(Parser, Debug)]
#[command(name = "clink")]
#[command(author = "link-foundation")]
#[command(version)]
#[command(about = "LiNo CLI Tool for managing links data store")]
struct Cli {
    /// Path to the links database file
    #[arg(short = 'd', long = "db", default_value = "db.links")]
    #[arg(alias = "data-source")]
    #[arg(alias = "data")]
    db: String,

    /// LiNo query for CRUD operation
    #[arg(short = 'q', long = "query")]
    #[arg(alias = "apply")]
    #[arg(alias = "do")]
    query: Option<String>,

    /// LiNo query for CRUD operation (positional argument)
    #[arg(name = "QUERY")]
    query_arg: Option<String>,

    /// Enable trace (verbose output)
    #[arg(short = 't', long = "trace", default_value = "false")]
    trace: bool,

    /// ID of the link to format its structure
    #[arg(short = 's', long = "structure")]
    structure: Option<u32>,

    /// Print the state of the database before applying changes
    #[arg(short = 'b', long = "before", default_value = "false")]
    before: bool,

    /// Print the changes applied by the query
    #[arg(short = 'c', long = "changes", default_value = "false")]
    changes: bool,

    /// Print the state of the database after applying changes
    #[arg(short = 'a', long = "after", default_value = "false")]
    #[arg(alias = "links")]
    after: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Create link storage
    let mut storage = LinkStorage::new(&cli.db, cli.trace)?;

    // If --structure is provided, handle it separately
    if let Some(link_id) = cli.structure {
        let structure_formatted = storage.format_structure(link_id)?;
        println!("{}", structure_formatted);
        return Ok(());
    }

    // Print before state if requested
    if cli.before {
        storage.print_all_links();
    }

    // Get effective query (option takes precedence over positional argument)
    let effective_query = cli.query.or(cli.query_arg);

    // Collect changes
    let mut changes_list = Vec::new();

    // Process query if provided
    if let Some(query) = effective_query {
        if !query.is_empty() {
            let processor = QueryProcessor::new(cli.trace);
            changes_list = processor.process_query(&mut storage, &query)?;
        }
    }

    // Print changes if requested
    if cli.changes && !changes_list.is_empty() {
        for (before_link, after_link) in &changes_list {
            storage.print_change(before_link, after_link);
        }
    }

    // Print after state if requested
    if cli.after {
        storage.print_all_links();
    }

    Ok(())
}
