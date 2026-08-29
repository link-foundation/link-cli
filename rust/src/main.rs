//! Link CLI - A command-line tool for links manipulation
//!
//! This is the Rust implementation of the link-cli tool, providing
//! similar functionality to the C# version.

use anyhow::{anyhow, bail, Result};
use link_cli::cli::{Cli, CliCommand};
use link_cli::import_lino_file;
use link_cli::{
    make_triggers_database_filename, CommitMode, LogRetentionPolicy, NamedTypeLinks,
    NamedTypesDecorator, PersistentTransformationDecorator, PersistentTransformationKind,
    QueryProcessor, TransactionsDecorator, TriggerStore, VersionControlDecorator,
};
use std::path::PathBuf;

fn main() -> Result<()> {
    let cli = match Cli::parse()? {
        CliCommand::Run(cli) => *cli,
        CliCommand::Help => {
            Cli::print_help();
            return Ok(());
        }
        CliCommand::Version => {
            println!("{}", Cli::version_text());
            return Ok(());
        }
    };

    if cli.trigger_command_count() > 1 {
        bail!("Only one of --always, --once, or --never can be used at a time.");
    }

    let vc_requested = cli.vc_requested();
    let transactions_requested = cli.transactions_requested();

    let commit_mode = parse_commit_mode(cli.commit_mode.as_deref())?;
    let retention_policy = parse_retention(cli.retention.as_deref())?;

    if vc_requested {
        run_with_vc(&cli, commit_mode, retention_policy)
    } else if transactions_requested {
        run_with_transactions(&cli, commit_mode, retention_policy)
    } else {
        run_bare(&cli)
    }
}

fn parse_commit_mode(raw: Option<&str>) -> Result<CommitMode> {
    match raw.map(|s| s.trim()).filter(|s| !s.is_empty()) {
        None => Ok(CommitMode::Sync),
        Some(value) if value.eq_ignore_ascii_case("sync") => Ok(CommitMode::Sync),
        Some(value) if value.eq_ignore_ascii_case("async") => Ok(CommitMode::Async),
        Some(other) => bail!("Invalid --commit-mode value '{other}'. Use 'sync' or 'async'."),
    }
}

fn parse_retention(raw: Option<&str>) -> Result<LogRetentionPolicy> {
    match raw.map(|s| s.trim()).filter(|s| !s.is_empty()) {
        None => Ok(LogRetentionPolicy::Infinite),
        Some(value) => LogRetentionPolicy::parse(value)
            .map_err(|e| anyhow!("Invalid --retention value '{value}': {e}")),
    }
}

fn run_bare(cli: &Cli) -> Result<()> {
    let storage = NamedTypesDecorator::new(&cli.db, cli.trace)?;
    finish(cli, storage)
}

/// Resolved path of the trigger sidecar store: `--triggers-file` when given,
/// `<db>.triggers.links` otherwise.
fn triggers_path(cli: &Cli) -> PathBuf {
    cli.triggers_file
        .clone()
        .map(PathBuf::from)
        .unwrap_or_else(|| make_triggers_database_filename(&cli.db))
}

/// Mirrors `persistentTransformationsEnabled` in the C# tool: any explicit flag
/// turns the layer on, and so does an already existing trigger store, so that
/// triggers stored by an earlier invocation keep firing without repeating
/// `--triggers`.
fn persistent_transformations_enabled(cli: &Cli) -> bool {
    cli.persistent_transformations_requested() || triggers_path(cli).exists()
}

/// Runs the query pipeline over `storage`, wrapping it in a
/// [`PersistentTransformationDecorator`] first when triggers are enabled.
///
/// The wrap happens here rather than in the three entry paths so that the
/// decorator always sits *outermost* — above the transactions and
/// version-control layers — exactly like in the C# tool, where every replayed
/// substitution is journalled like any other write.
fn finish<S>(cli: &Cli, storage: S) -> Result<()>
where
    S: NamedTypeLinks,
{
    if !persistent_transformations_enabled(cli) {
        let mut storage = storage;
        run_query_pipeline(cli, &mut storage)?;
        storage.save()?;
        return Ok(());
    }

    let trigger_store = if cli.embed_triggers {
        TriggerStore::Embedded
    } else {
        TriggerStore::sidecar(triggers_path(cli), cli.trace)?
    };
    let mut storage = PersistentTransformationDecorator::new(storage, trigger_store, cli.trace)
        .with_auto_create_missing_references(cli.auto_create_missing_references);

    run_query_pipeline_with(cli, &mut storage, run_trigger_command)?;
    storage.save()?;
    Ok(())
}

/// Handles `--always`, `--once` and `--never`.
///
/// Returns `true` when the command was terminal, i.e. the query was consumed as
/// a trigger definition and must not also be executed against the database.
fn run_trigger_command<S>(
    cli: &Cli,
    storage: &mut PersistentTransformationDecorator<S>,
    query: Option<&str>,
) -> Result<bool>
where
    S: NamedTypeLinks,
{
    if cli.trigger_command_count() == 0 {
        return Ok(false);
    }

    let query = query.filter(|query| !query.trim().is_empty());
    let Some(query) = query else {
        bail!("--always, --once, and --never require a query.");
    };

    if cli.always || cli.once {
        let kind = if cli.always {
            PersistentTransformationKind::Always
        } else {
            PersistentTransformationKind::Once
        };
        let root = storage.store_trigger(kind, query)?;
        println!("{kind} persistent transformation trigger stored: {root}");
    } else {
        let removed = storage.remove_triggers(query)?;
        println!("Persistent transformation triggers removed: {removed}");
    }

    Ok(true)
}

fn run_with_transactions(
    cli: &Cli,
    commit_mode: CommitMode,
    retention_policy: LogRetentionPolicy,
) -> Result<()> {
    let data_links = NamedTypesDecorator::new(&cli.db, cli.trace)?;
    let log_path = cli
        .transactions_file
        .clone()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| TransactionsDecorator::make_transitions_database_filename(&cli.db));
    let log_links = NamedTypesDecorator::new(&log_path, cli.trace)?;
    let mut tx = TransactionsDecorator::new(
        data_links,
        log_links,
        retention_policy,
        commit_mode,
        cli.trace,
    )?;

    if cli.show_log {
        for transition in tx.log() {
            println!(
                "{}\t{}\t{:?}\t{:032x}\t({},{},{}) -> ({},{},{})",
                transition.sequence,
                transition.timestamp_ms,
                transition.kind,
                transition.transaction_id,
                transition.before.index,
                transition.before.source,
                transition.before.target,
                transition.after.index,
                transition.after.source,
                transition.after.target,
            );
        }
        tx.save()?;
        return Ok(());
    }

    finish(cli, tx)
}

fn run_with_vc(
    cli: &Cli,
    commit_mode: CommitMode,
    retention_policy: LogRetentionPolicy,
) -> Result<()> {
    let data_links = NamedTypesDecorator::new(&cli.db, cli.trace)?;
    let log_path = cli
        .transactions_file
        .clone()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| TransactionsDecorator::make_transitions_database_filename(&cli.db));
    let log_links = NamedTypesDecorator::new(&log_path, cli.trace)?;
    let tx = TransactionsDecorator::new(
        data_links,
        log_links,
        retention_policy,
        commit_mode,
        cli.trace,
    )?;
    let vc_path = cli
        .vc_file
        .clone()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            VersionControlDecorator::make_version_control_database_filename(&cli.db)
        });
    let vc_links = NamedTypesDecorator::new(&vc_path, cli.trace)?;
    let mut vc = VersionControlDecorator::new(tx, vc_links, cli.trace)?;

    // 1) --checkout (resolves seq or tag).
    if let Some(checkout_point) = cli.checkout.as_deref() {
        let seq = resolve_sequence(&vc, checkout_point)
            .ok_or_else(|| anyhow!("Unknown checkout point '{checkout_point}'."))?;
        vc.checkout(seq)?;
        if cli.trace {
            println!("Checked out seq {seq} on branch '{}'.", vc.current_branch());
        }
    }

    // 2) --branch [--branch-from] (creates if missing, then switches).
    if let Some(branch_name) = cli.branch.as_deref() {
        let exists = vc.list_branches().iter().any(|b| b.name == branch_name);
        if !exists {
            vc.branch(branch_name, cli.branch_from)?;
            if cli.trace {
                println!("Created branch '{branch_name}'.");
            }
        }
        vc.switch_branch(branch_name)?;
        if cli.trace {
            println!("Switched to branch '{branch_name}'.");
        }
    }

    // 3) --tag.
    if let Some(tag_spec) = cli.tag.as_deref() {
        let (name, seq) = match tag_spec.find('=') {
            None => (tag_spec.to_string(), None),
            Some(eq) => {
                let (name_part, value_part) = tag_spec.split_at(eq);
                let value_part = &value_part[1..];
                let resolved = resolve_sequence(&vc, value_part)
                    .ok_or_else(|| anyhow!("Unknown tag point '{value_part}'."))?;
                (name_part.to_string(), Some(resolved))
            }
        };
        vc.tag(&name, seq)?;
        if cli.trace {
            let resolved = seq.unwrap_or_else(|| vc.current_sequence());
            println!("Tagged '{name}' at seq {resolved}.");
        }
    }

    // 4) --list-branches / --list-tags (terminal).
    if cli.list_branches {
        let current = vc.current_branch().to_string();
        for info in vc.list_branches() {
            let marker = if info.name == current { "*" } else { " " };
            let parent = info.parent.clone().unwrap_or_else(|| "-".to_string());
            println!(
                "{} {}\tparent={}\tfork={}\thead={}",
                marker, info.name, parent, info.fork_seq, info.head
            );
        }
        vc.save()?;
        return Ok(());
    }

    if cli.list_tags {
        for (name, seq) in vc.list_tags() {
            println!("{name}\t{seq}");
        }
        vc.save()?;
        return Ok(());
    }

    // 5) --log.
    if cli.show_log {
        for transition in vc.transactions().log() {
            println!(
                "{}\t{}\t{:?}\t{:032x}\t({},{},{}) -> ({},{},{})",
                transition.sequence,
                transition.timestamp_ms,
                transition.kind,
                transition.transaction_id,
                transition.before.index,
                transition.before.source,
                transition.before.target,
                transition.after.index,
                transition.after.source,
                transition.after.target,
            );
        }
        vc.save()?;
        return Ok(());
    }

    finish(cli, vc)
}

fn resolve_sequence(vc: &VersionControlDecorator, point: &str) -> Option<i64> {
    let trimmed = point.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(direct) = trimmed.parse::<i64>() {
        return Some(direct);
    }
    vc.try_get_tag(trimmed)
}

fn run_query_pipeline<S>(cli: &Cli, storage: &mut S) -> Result<()>
where
    S: NamedTypeLinks,
{
    run_query_pipeline_with(cli, storage, |_, _, _| Ok(false))
}

/// The query pipeline, with a hook that may consume the query before it reaches
/// the [`QueryProcessor`].
///
/// `trigger_stage` runs where the C# tool runs its trigger commands: after
/// `--structure`, so `clink --structure` keeps working unchanged, and before
/// the query is processed, so `--always '…'` stores the query instead of
/// applying it. Returning `true` ends the run after the LiNo output is written.
fn run_query_pipeline_with<S, F>(cli: &Cli, storage: &mut S, trigger_stage: F) -> Result<()>
where
    S: NamedTypeLinks,
    F: FnOnce(&Cli, &mut S, Option<&str>) -> Result<bool>,
{
    if cli.before {
        storage.print_all_lino()?;
    }

    if let Some(input_path) = &cli.lino_input {
        import_lino_file(storage, input_path)?;
    }

    if let Some(link_id) = cli.structure {
        let structure_formatted = storage.format_structure(link_id)?;
        println!("{structure_formatted}");
        if let Some(output_path) = &cli.lino_output {
            storage.write_lino_output(output_path)?;
        }
        return Ok(());
    }

    let effective_query = cli.query.as_deref().or(cli.query_arg.as_deref());

    if trigger_stage(cli, storage, effective_query)? {
        if let Some(output_path) = &cli.lino_output {
            storage.write_lino_output(output_path)?;
        }
        return Ok(());
    }

    let mut changes_list = Vec::new();

    if let Some(query) = effective_query {
        if !query.is_empty() {
            let processor = QueryProcessor::new(cli.trace)
                .with_auto_create_missing_references(cli.auto_create_missing_references);
            changes_list = processor.process_query(storage, query)?;
        }
    }

    if cli.changes && !changes_list.is_empty() {
        for (before_link, after_link) in &changes_list {
            storage.print_change_lino(before_link, after_link)?;
        }
    }

    if cli.after {
        storage.print_all_lino()?;
    }

    if let Some(output_path) = &cli.lino_output {
        storage.write_lino_output(output_path)?;
    }

    Ok(())
}
