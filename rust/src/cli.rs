//! Command-line argument parsing for the `clink` binary.

use anyhow::{bail, Result};
use std::env;
use std::ffi::OsString;

const DEFAULT_DATABASE_FILENAME: &str = "db.links";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cli {
    pub db: String,
    pub query: Option<String>,
    pub query_arg: Option<String>,
    pub trace: bool,
    pub auto_create_missing_references: bool,
    pub structure: Option<u32>,
    pub before: bool,
    pub changes: bool,
    pub after: bool,
    pub lino_input: Option<String>,
    pub lino_output: Option<String>,
    pub transactions: bool,
    pub transactions_file: Option<String>,
    pub commit_mode: Option<String>,
    pub retention: Option<String>,
    pub vc: bool,
    pub vc_file: Option<String>,
    pub branch: Option<String>,
    pub branch_from: Option<i64>,
    pub checkout: Option<String>,
    pub tag: Option<String>,
    pub list_branches: bool,
    pub list_tags: bool,
    pub show_log: bool,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            db: DEFAULT_DATABASE_FILENAME.to_string(),
            query: None,
            query_arg: None,
            trace: false,
            auto_create_missing_references: false,
            structure: None,
            before: false,
            changes: false,
            after: false,
            lino_input: None,
            lino_output: None,
            transactions: false,
            transactions_file: None,
            commit_mode: None,
            retention: None,
            vc: false,
            vc_file: None,
            branch: None,
            branch_from: None,
            checkout: None,
            tag: None,
            list_branches: false,
            list_tags: false,
            show_log: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliCommand {
    Run(Box<Cli>),
    Help,
    Version,
}

impl Cli {
    /// True when any flag in the transactions decorator family was passed.
    pub fn transactions_requested(&self) -> bool {
        self.transactions
            || self.transactions_file.is_some()
            || self.commit_mode.is_some()
            || self.retention.is_some()
            || self.show_log
            || self.vc_requested()
    }

    /// True when any flag in the version-control decorator family was passed.
    pub fn vc_requested(&self) -> bool {
        self.vc
            || self.vc_file.is_some()
            || self.branch.is_some()
            || self.branch_from.is_some()
            || self.checkout.is_some()
            || self.tag.is_some()
            || self.list_branches
            || self.list_tags
    }

    pub fn parse() -> Result<CliCommand> {
        lino_arguments::init();
        Self::parse_from(env::args_os())
    }

    pub fn parse_from<I, T>(args: I) -> Result<CliCommand>
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString>,
    {
        let mut cli = Cli::default();
        let mut args = args
            .into_iter()
            .map(|arg| arg.into().to_string_lossy().into_owned())
            .peekable();

        let _program = args.next();

        while let Some(arg) = args.next() {
            if let Some(value) = inline_value(&arg, &["--db", "--data-source", "--data"]) {
                cli.db = value.to_string();
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--query", "--apply", "--do"]) {
                cli.query = Some(value.to_string());
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--structure"]) {
                cli.structure = Some(parse_link_id("--structure", value)?);
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--trace"]) {
                cli.trace = parse_bool("--trace", value)?;
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--auto-create-missing-references"]) {
                cli.auto_create_missing_references =
                    parse_bool("--auto-create-missing-references", value)?;
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--before"]) {
                cli.before = parse_bool("--before", value)?;
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--changes"]) {
                cli.changes = parse_bool("--changes", value)?;
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--after", "--links"]) {
                cli.after = parse_bool("--after", value)?;
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--out", "--lino-output", "--export"]) {
                cli.lino_output = Some(value.to_string());
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--in", "--lino-input", "--import"]) {
                cli.lino_input = Some(value.to_string());
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--transactions"]) {
                cli.transactions = parse_bool("--transactions", value)?;
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--transactions-file"]) {
                cli.transactions_file = Some(value.to_string());
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--commit-mode"]) {
                cli.commit_mode = Some(value.to_string());
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--retention"]) {
                cli.retention = Some(value.to_string());
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--vc"]) {
                cli.vc = parse_bool("--vc", value)?;
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--vc-file"]) {
                cli.vc_file = Some(value.to_string());
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--branch"]) {
                cli.branch = Some(value.to_string());
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--branch-from"]) {
                cli.branch_from = Some(parse_seq("--branch-from", value)?);
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--checkout"]) {
                cli.checkout = Some(value.to_string());
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--tag"]) {
                cli.tag = Some(value.to_string());
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--list-branches"]) {
                cli.list_branches = parse_bool("--list-branches", value)?;
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--list-tags"]) {
                cli.list_tags = parse_bool("--list-tags", value)?;
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--log"]) {
                cli.show_log = parse_bool("--log", value)?;
                continue;
            }

            match arg.as_str() {
                "-h" | "--help" => return Ok(CliCommand::Help),
                "-V" | "--version" => return Ok(CliCommand::Version),
                "-d" | "--db" | "--data-source" | "--data" => {
                    cli.db = next_value(&mut args, &arg)?;
                }
                "-q" | "--query" | "--apply" | "--do" => {
                    cli.query = Some(next_value(&mut args, &arg)?);
                }
                "-t" | "--trace" => {
                    cli.trace = next_bool_value(&mut args, true)?;
                }
                "--auto-create-missing-references" => {
                    cli.auto_create_missing_references = next_bool_value(&mut args, true)?;
                }
                "-s" | "--structure" => {
                    let value = next_value(&mut args, &arg)?;
                    cli.structure = Some(parse_link_id(&arg, &value)?);
                }
                "-b" | "--before" => {
                    cli.before = next_bool_value(&mut args, true)?;
                }
                "-c" | "--changes" => {
                    cli.changes = next_bool_value(&mut args, true)?;
                }
                "-a" | "--after" | "--links" => {
                    cli.after = next_bool_value(&mut args, true)?;
                }
                "--out" | "--lino-output" | "--export" => {
                    cli.lino_output = Some(next_value(&mut args, &arg)?);
                }
                "--in" | "--lino-input" | "--import" => {
                    cli.lino_input = Some(next_value(&mut args, &arg)?);
                }
                "--transactions" => {
                    cli.transactions = next_bool_value(&mut args, true)?;
                }
                "--transactions-file" => {
                    cli.transactions_file = Some(next_value(&mut args, &arg)?);
                }
                "--commit-mode" => {
                    cli.commit_mode = Some(next_value(&mut args, &arg)?);
                }
                "--retention" => {
                    cli.retention = Some(next_value(&mut args, &arg)?);
                }
                "--vc" => {
                    cli.vc = next_bool_value(&mut args, true)?;
                }
                "--vc-file" => {
                    cli.vc_file = Some(next_value(&mut args, &arg)?);
                }
                "--branch" => {
                    cli.branch = Some(next_value(&mut args, &arg)?);
                }
                "--branch-from" => {
                    let value = next_value(&mut args, &arg)?;
                    cli.branch_from = Some(parse_seq(&arg, &value)?);
                }
                "--checkout" => {
                    cli.checkout = Some(next_value(&mut args, &arg)?);
                }
                "--tag" => {
                    cli.tag = Some(next_value(&mut args, &arg)?);
                }
                "--list-branches" => {
                    cli.list_branches = next_bool_value(&mut args, true)?;
                }
                "--list-tags" => {
                    cli.list_tags = next_bool_value(&mut args, true)?;
                }
                "--log" => {
                    cli.show_log = next_bool_value(&mut args, true)?;
                }
                "--" => {
                    for value in args.by_ref() {
                        set_positional_query(&mut cli, value)?;
                    }
                    break;
                }
                value if value.starts_with('-') => {
                    bail!("unknown option '{value}'");
                }
                value => {
                    set_positional_query(&mut cli, value.to_string())?;
                }
            }
        }

        Ok(CliCommand::Run(Box::new(cli)))
    }

    pub fn print_help() {
        print!("{}", Self::help_text());
    }

    pub fn help_text() -> &'static str {
        concat!(
            "LiNo CLI Tool for managing links data store\n\n",
            "Usage: clink [OPTIONS] [QUERY]\n\n",
            "Arguments:\n",
            "  [QUERY]  LiNo query for CRUD operation\n\n",
            "Options:\n",
            "  -d, --db <DB>, --data-source <DB>, --data <DB>\n",
            "          Path to the links database file [default: db.links]\n",
            "  -q, --query <QUERY>, --apply <QUERY>, --do <QUERY>\n",
            "          LiNo query for CRUD operation\n",
            "  -t, --trace\n",
            "          Enable trace (verbose output)\n",
            "      --auto-create-missing-references\n",
            "          Create missing numeric and named references as self-referential point links\n",
            "  -s, --structure <STRUCTURE>\n",
            "          ID of the link to format its structure\n",
            "  -b, --before\n",
            "          Print the state of the database before applying changes\n",
            "  -c, --changes\n",
            "          Print the changes applied by the query\n",
            "  -a, --after, --links\n",
            "          Print the state of the database after applying changes\n",
            "      --in <IN>, --lino-input <IN>, --import <IN>\n",
            "          Read and import a LiNo file into the database\n",
            "      --out <OUT>, --lino-output <OUT>, --export <OUT>\n",
            "          Write the complete database as a LiNo file\n",
            "      --transactions\n",
            "          Enable the transactions layer (default log path: <db>.transitions.links)\n",
            "      --transactions-file <FILE>\n",
            "          Path to the transitions log store (implies --transactions)\n",
            "      --commit-mode <MODE>\n",
            "          Choose 'sync' or 'async' commits (default: sync, implies --transactions)\n",
            "      --retention <SPEC>\n",
            "          Log retention policy: 'infinite', 'sized:<n>', or 'chunked:<n>:<dir>'\n",
            "          (implies --transactions)\n",
            "      --vc\n",
            "          Enable the version-control decorator (implies --transactions)\n",
            "      --vc-file <FILE>\n",
            "          Path to the version-control branches store\n",
            "          (default: <db>.versioncontrol.links)\n",
            "      --branch <NAME>\n",
            "          Switch to a branch (creating it if --branch-from is also passed).\n",
            "          Implies --vc.\n",
            "      --branch-from <SEQ>\n",
            "          When creating a branch with --branch, fork from this sequence point\n",
            "      --checkout <POINT>\n",
            "          Time-travel to a specific transition sequence or named tag.\n",
            "          Implies --vc.\n",
            "      --tag <NAME[=SEQ]>\n",
            "          Create a tag at current head or at the given sequence point.\n",
            "          Implies --vc.\n",
            "      --list-branches\n",
            "          List version-control branches and exit\n",
            "      --list-tags\n",
            "          List version-control tags and exit\n",
            "      --log\n",
            "          Print the transitions log and exit (implies --transactions)\n",
            "  -h, --help\n",
            "          Print help\n",
            "  -V, --version\n",
            "          Print version\n",
        )
    }

    pub fn version_text() -> String {
        format!("clink {}", env!("CARGO_PKG_VERSION"))
    }
}

fn inline_value<'a>(arg: &'a str, names: &[&str]) -> Option<&'a str> {
    names.iter().find_map(|name| {
        arg.strip_prefix(name)
            .and_then(|rest| rest.strip_prefix('='))
    })
}

fn next_value<I>(args: &mut I, option: &str) -> Result<String>
where
    I: Iterator<Item = String>,
{
    args.next()
        .ok_or_else(|| anyhow::anyhow!("missing value for option '{option}'"))
}

fn next_bool_value<I>(args: &mut std::iter::Peekable<I>, default: bool) -> Result<bool>
where
    I: Iterator<Item = String>,
{
    if let Some(value) = args.peek().and_then(|value| bool_literal(value)) {
        args.next();
        Ok(value)
    } else {
        Ok(default)
    }
}

fn parse_bool(option: &str, value: &str) -> Result<bool> {
    bool_literal(value)
        .ok_or_else(|| anyhow::anyhow!("invalid boolean value '{value}' for {option}"))
}

fn bool_literal(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn parse_link_id(option: &str, value: &str) -> Result<u32> {
    value
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid link id '{value}' for {option}"))
}

fn parse_seq(option: &str, value: &str) -> Result<i64> {
    value
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid sequence value '{value}' for {option}"))
}

fn set_positional_query(cli: &mut Cli, value: String) -> Result<()> {
    if cli.query_arg.is_some() {
        bail!("unexpected extra positional argument '{value}'");
    }

    cli.query_arg = Some(value);
    Ok(())
}
