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
    pub lino_output: Option<String>,
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
            lino_output: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliCommand {
    Run(Cli),
    Help,
    Version,
}

impl Cli {
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

        Ok(CliCommand::Run(cli))
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
            "      --out <OUT>, --lino-output <OUT>, --export <OUT>\n",
            "          Write the complete database as a LiNo file\n",
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

fn set_positional_query(cli: &mut Cli, value: String) -> Result<()> {
    if cli.query_arg.is_some() {
        bail!("unexpected extra positional argument '{value}'");
    }

    cli.query_arg = Some(value);
    Ok(())
}
