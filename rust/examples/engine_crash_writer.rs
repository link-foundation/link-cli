//! A writer to be killed: `.github/scripts/docker-crash.mjs` runs it in a
//! container, sends SIGKILL at a random moment, damages its files and repairs
//! them. The C# (examples/engine-crash-csharp) and JS
//! (js/client/examples/engine-crash-writer.mjs) writers speak the same protocol:
//!
//!   write  PRIMARY REPLICA [COMMITS]  print `ack R` once revision R is durable
//!                                     on both files; `error ...` and exit 1 on
//!                                     a failure such as a full disk
//!   verify JOURNAL                    open it as a restarting writer does and
//!                                     print `revision R`, or `refused ...` and
//!                                     exit 2
//!   repair JOURNAL [COPY...]          rebuild it from its valid prefix or a
//!                                     copy: `repaired R changed true|false`
//!
//! Revision R holds links 1..=R, each pointing at itself. Each commit creates a
//! link and then points it at itself, so a torn transaction shows as a link
//! pointing at nothing, and `verify` refuses it.
use link_cli::engine::recovery::repair_journal_file;
use link_cli::engine::replication::{ReplicatedLog, Synchronization};
use link_cli::engine::{Engine, EngineOptions, RevisionEncoding};
use link_cli::{FileTransitionLog, LinksStorage};
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

fn options() -> EngineOptions {
    EngineOptions {
        revision_encoding: RevisionEncoding::Changes,
        require_durable_flush: true,
        ..EngineOptions::default()
    }
}

fn write(primary: &str, replica: &str, commits: Option<u64>) -> Result<(), String> {
    let log = ReplicatedLog::new(
        FileTransitionLog::open_exclusive(primary).map_err(|e| e.to_string())?,
        vec![FileTransitionLog::open_exclusive(replica).map_err(|e| e.to_string())?],
        Synchronization::Synchronous {
            acknowledgements: 2,
        },
    )
    .map_err(|e| e.to_string())?;
    let db = Engine::<u64, _>::open(log, options()).map_err(|e| e.to_string())?;
    let mut stdout = std::io::stdout();
    let mut written = 0;
    while commits.is_none_or(|limit| written < limit) {
        let revision = (|| {
            let mut tx = db.begin()?;
            let id = tx.create_link(0, 0)?;
            tx.update_link(id, id, id)?;
            tx.commit()
        })()
        .map_err(|e| e.to_string())?;
        writeln!(stdout, "ack {revision}")
            .and_then(|()| stdout.flush())
            .map_err(|e| e.to_string())?;
        written += 1;
    }
    Ok(())
}

fn verify(path: &str) -> Result<u64, String> {
    let log = FileTransitionLog::open_exclusive(path).map_err(|e| e.to_string())?;
    let db = Engine::<u64, _>::open(log, options()).map_err(|e| e.to_string())?;
    let snapshot = db.snapshot().map_err(|e| e.to_string())?;
    let links = snapshot.links();
    if links.len() as u64 != snapshot.revision {
        return Err(format!(
            "{} links at revision {}",
            links.len(),
            snapshot.revision
        ));
    }
    for (expected, link) in (1..).zip(&links) {
        if (link.index, link.source, link.target) != (expected, expected, expected) {
            return Err(format!(
                "torn transaction: link {}:{}:{} where {expected}:{expected}:{expected} was committed",
                link.index, link.source, link.target
            ));
        }
    }
    Ok(snapshot.revision)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let outcome = match args.as_slice() {
        ["write", primary, replica, rest @ ..] if rest.len() <= 1 => {
            let commits = rest.first().map(|count| count.parse::<u64>());
            match commits.transpose() {
                Ok(commits) => write(primary, replica, commits).map(|()| String::new()),
                Err(error) => Err(error.to_string()),
            }
        }
        ["verify", path] => match verify(path) {
            Ok(revision) => Ok(format!("revision {revision}")),
            Err(reason) => {
                println!("refused {reason}");
                return ExitCode::from(2);
            }
        },
        ["repair", path, copies @ ..] => {
            let copies: Vec<&Path> = copies.iter().map(Path::new).collect();
            repair_journal_file::<u64>(Path::new(path), &copies, &options())
                .map(|repair| {
                    format!(
                        "repaired {} changed {}",
                        repair.recovery.revision, repair.changed
                    )
                })
                .map_err(|e| e.to_string())
        }
        _ => Err("usage: engine_crash_writer write PRIMARY REPLICA [COMMITS] | verify JOURNAL | repair JOURNAL [COPY...]".into()),
    };
    match outcome {
        Ok(line) => {
            if !line.is_empty() {
                println!("{line}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            println!("error {error}");
            ExitCode::FAILURE
        }
    }
}
