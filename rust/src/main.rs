//! Link CLI - A command-line tool for links manipulation
//!
//! This is the Rust implementation of the link-cli tool, providing
//! similar functionality to the C# version.

use anyhow::Result;
use link_cli::cli::{Cli, CliCommand};
use link_cli::{LinkStorage, QueryProcessor};

fn main() -> Result<()> {
    let cli = match Cli::parse()? {
        CliCommand::Run(cli) => cli,
        CliCommand::Help => {
            Cli::print_help();
            return Ok(());
        }
        CliCommand::Version => {
            println!("{}", Cli::version_text());
            return Ok(());
        }
    };

    // Create link storage
    let mut storage = LinkStorage::new(&cli.db, cli.trace)?;

    // If --structure is provided, handle it separately
    if let Some(link_id) = cli.structure {
        let structure_formatted = storage.format_structure(link_id)?;
        println!("{}", structure_formatted);
        if let Some(output_path) = &cli.lino_output {
            storage.write_lino_output(output_path)?;
        }
        return Ok(());
    }

    // Print before state if requested
    if cli.before {
        storage.print_all_links();
    }

    // Get effective query (option takes precedence over positional argument)
    let effective_query = cli.query.as_deref().or(cli.query_arg.as_deref());

    // Collect changes
    let mut changes_list = Vec::new();

    // Process query if provided
    if let Some(query) = effective_query {
        if !query.is_empty() {
            let processor = QueryProcessor::new(cli.trace);
            changes_list = processor.process_query(&mut storage, query)?;
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

    if let Some(output_path) = &cli.lino_output {
        storage.write_lino_output(output_path)?;
    }

    Ok(())
}
