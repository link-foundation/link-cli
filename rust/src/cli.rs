//! Command-line argument parsing for the `clink` binary.

use crate::protocol::ArityRange;
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
    pub indexed_lookups: bool,
    pub auto_create_missing_references: bool,
    pub structure: Option<u32>,
    pub before: bool,
    pub changes: bool,
    pub after: bool,
    pub lino_input: Option<String>,
    pub lino_output: Option<String>,
    pub binary_input: Option<String>,
    pub binary_output: Option<String>,
    pub always: bool,
    pub once: bool,
    pub never: bool,
    pub triggers: bool,
    pub triggers_file: Option<String>,
    pub embed_triggers: bool,
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
    /// The first `--serve` endpoint, such as `127.0.0.1:7878` or
    /// `unix:/tmp/links.sock` (issue #140).
    pub serve: Option<String>,
    /// Every later `--serve` endpoint: one server listens on all of them.
    pub serve_also: Vec<String>,
    /// `--forward`: relay each `--serve` connection to this endpoint, byte
    /// for byte, instead of serving a database (issue #140).
    pub forward: Option<String>,
    /// `--graphql`: `--serve` speaks Hasura's GraphQL for Deep (issue #153).
    pub graphql: bool,
    /// `--jwt-secret`: signs the tokens of `--graphql`; random when absent.
    pub jwt_secret: Option<String>,
    /// `--graphql-admin-secret`: what `x-hasura-admin-secret` must be.
    pub graphql_admin_secret: Option<String>,
    /// `--deep-package`: Deep links, as JSON rows, to load before serving.
    pub deep_packages: Vec<String>,
    pub connect: Option<String>,
    pub protocol: Option<String>,
    pub external_references: bool,
    pub arity: ArityRange,
    pub packed_widths: bool,
    /// `--triplets`: `--serve` reads every request as triplets (issue #152).
    pub triplets: bool,
    /// `--permissions`: `self` or a links file of permission rules (issue #149).
    pub permissions: Option<String>,
    /// `--subject`: the link index or name queries run as.
    pub subject: Option<String>,
    /// `--tokens`: `self` or a links file of the token table.
    pub tokens: Option<String>,
    pub grant_token: Option<String>,
    pub revoke_token: Option<String>,
    /// `--token`: authenticates a `--connect` client.
    pub token: Option<String>,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            db: DEFAULT_DATABASE_FILENAME.to_string(),
            query: None,
            query_arg: None,
            trace: false,
            indexed_lookups: false,
            auto_create_missing_references: false,
            structure: None,
            before: false,
            changes: false,
            after: false,
            lino_input: None,
            lino_output: None,
            binary_input: None,
            binary_output: None,
            always: false,
            once: false,
            never: false,
            triggers: false,
            triggers_file: None,
            embed_triggers: false,
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
            serve: None,
            serve_also: Vec::new(),
            forward: None,
            graphql: false,
            jwt_secret: None,
            graphql_admin_secret: None,
            deep_packages: Vec::new(),
            connect: None,
            protocol: None,
            external_references: false,
            arity: ArityRange::DOUBLETS,
            packed_widths: false,
            triplets: false,
            permissions: None,
            subject: None,
            tokens: None,
            grant_token: None,
            revoke_token: None,
            token: None,
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

    /// True when a trigger command — `--always`, `--once` or `--never` — was
    /// passed. Exactly one of them may be used at a time.
    pub fn trigger_command_count(&self) -> usize {
        usize::from(self.always) + usize::from(self.once) + usize::from(self.never)
    }

    /// True when any flag in the persistent transformation family was passed.
    ///
    /// Mirrors `persistentTransformationsEnabled` in the C# tool, minus the
    /// "the triggers file already exists" clause, which needs the resolved
    /// path and therefore lives next to it in `main`.
    pub fn persistent_transformations_requested(&self) -> bool {
        self.always
            || self.once
            || self.never
            || self.triggers
            || self.embed_triggers
            || self.triggers_file.is_some()
    }

    /// Every `--serve` endpoint, in the order given.
    pub fn serve_endpoints(&self) -> Vec<&str> {
        self.serve
            .iter()
            .chain(&self.serve_also)
            .map(String::as_str)
            .collect()
    }

    fn add_serve(&mut self, endpoint: String) {
        if self.serve.is_none() {
            self.serve = Some(endpoint);
        } else {
            self.serve_also.push(endpoint);
        }
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
            if let Some(value) = inline_value(&arg, &["--indexed-lookups"]) {
                cli.indexed_lookups = parse_bool("--indexed-lookups", value)?;
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
            if let Some(value) = inline_value(
                &arg,
                &["--export-binary", "--binary-output", "--binary-out"],
            ) {
                cli.binary_output = Some(value.to_string());
                continue;
            }
            if let Some(value) =
                inline_value(&arg, &["--import-binary", "--binary-input", "--binary-in"])
            {
                cli.binary_input = Some(value.to_string());
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--always"]) {
                cli.always = parse_bool("--always", value)?;
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--once"]) {
                cli.once = parse_bool("--once", value)?;
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--never"]) {
                cli.never = parse_bool("--never", value)?;
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--triggers"]) {
                cli.triggers = parse_bool("--triggers", value)?;
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--triggers-file"]) {
                cli.triggers_file = Some(value.to_string());
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--embed-triggers"]) {
                cli.embed_triggers = parse_bool("--embed-triggers", value)?;
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
            if let Some(value) = inline_value(&arg, &["--jwt-secret"]) {
                cli.jwt_secret = Some(value.to_string());
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--graphql-admin-secret"]) {
                cli.graphql_admin_secret = Some(value.to_string());
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--deep-package"]) {
                cli.deep_packages.push(value.to_string());
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--serve"]) {
                cli.add_serve(value.to_string());
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--forward"]) {
                cli.forward = Some(value.to_string());
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--connect"]) {
                cli.connect = Some(value.to_string());
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--protocol"]) {
                cli.protocol = Some(value.to_string());
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--external-references"]) {
                cli.external_references = parse_bool("--external-references", value)?;
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--arity"]) {
                cli.arity = parse_arity(value)?;
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--packed-widths"]) {
                cli.packed_widths = parse_bool("--packed-widths", value)?;
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--triplets"]) {
                cli.triplets = parse_bool("--triplets", value)?;
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--permissions"]) {
                cli.permissions = Some(value.to_string());
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--subject"]) {
                cli.subject = Some(value.to_string());
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--tokens"]) {
                cli.tokens = Some(value.to_string());
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--grant-token"]) {
                cli.grant_token = Some(value.to_string());
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--revoke-token"]) {
                cli.revoke_token = Some(value.to_string());
                continue;
            }
            if let Some(value) = inline_value(&arg, &["--token"]) {
                cli.token = Some(value.to_string());
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
                "--indexed-lookups" => {
                    cli.indexed_lookups = next_bool_value(&mut args, true)?;
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
                "--export-binary" | "--binary-output" | "--binary-out" => {
                    cli.binary_output = Some(next_value(&mut args, &arg)?);
                }
                "--import-binary" | "--binary-input" | "--binary-in" => {
                    cli.binary_input = Some(next_value(&mut args, &arg)?);
                }
                "--always" => {
                    cli.always = next_bool_value(&mut args, true)?;
                }
                "--once" => {
                    cli.once = next_bool_value(&mut args, true)?;
                }
                "--never" => {
                    cli.never = next_bool_value(&mut args, true)?;
                }
                "--triggers" => {
                    cli.triggers = next_bool_value(&mut args, true)?;
                }
                "--triggers-file" => {
                    cli.triggers_file = Some(next_value(&mut args, &arg)?);
                }
                "--embed-triggers" => {
                    cli.embed_triggers = next_bool_value(&mut args, true)?;
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
                "--graphql" => {
                    cli.graphql = next_bool_value(&mut args, true)?;
                }
                "--jwt-secret" => {
                    cli.jwt_secret = Some(next_value(&mut args, &arg)?);
                }
                "--graphql-admin-secret" => {
                    cli.graphql_admin_secret = Some(next_value(&mut args, &arg)?);
                }
                "--deep-package" => {
                    cli.deep_packages.push(next_value(&mut args, &arg)?);
                }
                "--serve" => {
                    let value = next_value(&mut args, &arg)?;
                    cli.add_serve(value);
                }
                "--forward" => {
                    cli.forward = Some(next_value(&mut args, &arg)?);
                }
                "--connect" => {
                    cli.connect = Some(next_value(&mut args, &arg)?);
                }
                "--protocol" => {
                    cli.protocol = Some(next_value(&mut args, &arg)?);
                }
                "--external-references" => {
                    cli.external_references = next_bool_value(&mut args, true)?;
                }
                "--arity" => {
                    cli.arity = parse_arity(&next_value(&mut args, &arg)?)?;
                }
                "--packed-widths" => {
                    cli.packed_widths = next_bool_value(&mut args, true)?;
                }
                "--triplets" => {
                    cli.triplets = next_bool_value(&mut args, true)?;
                }
                "--permissions" => {
                    cli.permissions = Some(next_value(&mut args, &arg)?);
                }
                "--subject" => {
                    cli.subject = Some(next_value(&mut args, &arg)?);
                }
                "--tokens" => {
                    cli.tokens = Some(next_value(&mut args, &arg)?);
                }
                "--grant-token" => {
                    cli.grant_token = Some(next_value(&mut args, &arg)?);
                }
                "--revoke-token" => {
                    cli.revoke_token = Some(next_value(&mut args, &arg)?);
                }
                "--token" => {
                    cli.token = Some(next_value(&mut args, &arg)?);
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
            "      --indexed-lookups\n",
            "          Use optional lookup-index decorators for local links and names (Rust only)\n",
            "  -t, --trace\n",
            "          Enable trace (verbose output), including each transport event of\n",
            "          --serve, --connect and --forward (as CLINK_IPC_TRACE=1 does)\n",
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
            "      --import-binary <PATH>, --binary-input <PATH>, --binary-in <PATH>\n",
            "          Read and import a store archive in binary links notation\n",
            "      --export-binary <PATH>, --binary-output <PATH>, --binary-out <PATH>\n",
            "          Write the complete database, names included, as a store archive in\n",
            "          binary links notation\n",
            "      --always\n",
            "          Store the query as an always-on persistent transformation trigger\n",
            "      --once\n",
            "          Store the query as a persistent transformation trigger that deletes\n",
            "          itself after it fires\n",
            "      --never\n",
            "          Remove stored persistent transformation triggers matching the query\n",
            "      --triggers\n",
            "          Enable persistent transformation triggers for this command\n",
            "      --triggers-file <FILE>\n",
            "          Path to the persistent transformation trigger links database\n",
            "          (default: <db>.triggers.links)\n",
            "      --embed-triggers\n",
            "          Store persistent transformation triggers directly in the main links\n",
            "          database\n",
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
            "      --tag <NAME[=POINT]>\n",
            "          Create a tag at current head or at a sequence/existing tag.\n",
            "          Uses the first '=' whose complete suffix resolves.\n",
            "          Implies --vc.\n",
            "      --list-branches\n",
            "          List version-control branches and exit\n",
            "      --list-tags\n",
            "          List version-control tags and exit\n",
            "      --log\n",
            "          Print the transitions log and exit (implies --transactions)\n",
            "      --serve <ADDR>\n",
            "          Serve the database on an endpoint until stopped: a TCP address\n",
            "          (e.g. 127.0.0.1:7878) or KIND:ADDRESS for another transport, such as\n",
            "          unix:/tmp/links.sock, pipe:NAME, fifo:DIR, shm:DIR, mmap:DIR, file:DIR,\n",
            "          signal:DIR, mq:DIR, sysvmq:DIR, udp:HOST:PORT, stdio, mailslot:DIR,\n",
            "          copydata:DIR or alpc:DIR (see docs/protocol/ipc-transports-v1.md);\n",
            "          repeat it to serve one store on several endpoints at once. Each\n",
            "          message may use the text or the binary LiNo protocol\n",
            "      --forward <ADDR>\n",
            "          With --serve: relay every connection, byte for byte, to this\n",
            "          endpoint instead of serving a database (stacks transports)\n",
            "      --graphql\n",
            "          With --serve: speak Hasura's GraphQL over HTTP and WebSocket for Deep\n",
            "      --jwt-secret <SECRET>\n",
            "          Signs the tokens of --graphql (default: random for each run)\n",
            "      --graphql-admin-secret <SECRET>\n",
            "          The x-hasura-admin-secret that bypasses --permissions in --graphql\n",
            "      --deep-package <FILE>\n",
            "          Load a Deep package (JSON rows of links) before --graphql serves\n",
            "      --connect <ADDR>\n",
            "          Send the query to a clink server on a --serve endpoint (e.g.\n",
            "          127.0.0.1:7878 or shm:/tmp/links) instead of opening a database;\n",
            "          an empty query reads every link\n",
            "      --protocol <PROTOCOL>\n",
            "          'text' or 'binary' for --connect (default: text); 'text', 'binary'\n",
            "          or 'any' for --serve (default: any)\n",
            "      --external-references\n",
            "          Binary protocol: send numbers and characters as external references\n",
            "      --arity <RANGE>\n",
            "          Binary protocol: link lengths to use, 'n', 'min..max' or 'min..'\n",
            "          (default: 2, doublets only; e.g. 2..3 adds triplets, 1.. any length)\n",
            "      --packed-widths\n",
            "          Binary protocol: give each section of links the narrowest reference\n",
            "          width it needs instead of one width for the whole packet\n",
            "      --triplets\n",
            "          With --serve: read every request as triplets, (create: (type from to)),\n",
            "          (each: (index type from to)) and three-part substitutions, over this\n",
            "          store of doublets; without it, every request is about doublets\n",
            "      --permissions <RULES>\n",
            "          Check every read and write against permission rules stored as links:\n",
            "          'self' (rules in the data) or a links file of rules; without it,\n",
            "          nothing is checked\n",
            "      --subject <LINK>\n",
            "          The subject (a link index or name) queries run as with --permissions,\n",
            "          and the subject of --serve connections before (auth: token)\n",
            "          (default: 0, a guest)\n",
            "      --tokens <TOKENS>\n",
            "          The token table of (auth: token) for --serve, --grant-token and\n",
            "          --revoke-token: 'self' (tokens in the data) or a links file\n",
            "      --grant-token <TOKEN>\n",
            "          Let a token authenticate as --subject in the --tokens table, then exit\n",
            "      --revoke-token <TOKEN>\n",
            "          Forget a token of the --tokens table, then exit\n",
            "      --token <TOKEN>\n",
            "          With --connect: authenticate with (auth: token) before sending the query\n",
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

fn parse_arity(value: &str) -> Result<ArityRange> {
    value
        .trim()
        .parse()
        .map_err(|error| anyhow::anyhow!("invalid value for '--arity': {error}"))
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
