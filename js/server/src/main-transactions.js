// The transactions and version-control entry points of rust/src/main.rs:
// `parse_commit_mode`, `parse_retention`, `run_with_transactions`,
// `run_with_vc` and `resolve_sequence`.
//
// `runWithTransactions` and `runWithVc` take the pipeline from main.js in
// `context`: `finish(cli, storage)` runs the query pipeline over a store,
// `openNamedStorage(path, cli)` opens a `NamedTypesDecorator`,
// `openJournal(database, explicit)` opens the transitions journal (main.js
// passes `openTransitionsJournal` of transactions-journal-file.js), and `out`
// and `err` are the standard streams.
import { trim } from './text-storage-format.js';
import { TransactionsDecorator } from './transactions.js';
import {
  CommitMode,
  hex32,
  INFINITE_RETENTION,
  KIND_NAMES,
  parseI64,
  parseRetentionPolicy,
} from './transactions-types.js';
import { makeVersionControlDatabaseFilename, VersionControlDecorator } from './version-control.js';

/** `--commit-mode`: `sync` (the default) or `async`, in any ASCII case. */
export function parseCommitMode(raw) {
  const value = raw == null ? '' : trim(raw);
  if (value === '') return CommitMode.Sync;
  const lowered = value.replace(/[A-Z]+/g, (letters) => letters.toLowerCase());
  if (lowered === 'sync') return CommitMode.Sync;
  if (lowered === 'async') return CommitMode.Async;
  throw new Error(`Invalid --commit-mode value '${value}'. Use 'sync' or 'async'.`);
}

/** `--retention`: the log retention policy, infinite by default. */
export function parseRetention(raw) {
  const value = raw == null ? '' : trim(raw);
  if (value === '') return INFINITE_RETENTION;
  try {
    return parseRetentionPolicy(value);
  } catch (error) {
    // `anyhow!` formats the cause into a new error with no source, so the
    // reported chain must not repeat it.
    // eslint-disable-next-line preserve-caught-error
    throw new Error(`Invalid --retention value '${value}': ${error.message}`);
  }
}

/** A `--checkout` or `--tag` point: a sequence number or a tag name, or `null`. */
export function resolveSequence(vc, point) {
  const trimmed = trim(point);
  if (trimmed === '') return null;
  return parseI64(trimmed) ?? vc.tryGetTag(trimmed);
}

/** Prints the transitions log, one tab-separated line per transition. */
function printLog(transactions, out) {
  for (const { sequence, timestampMs, kind, transactionId, before, after } of transactions.log()) {
    out.write(
      `${sequence}\t${timestampMs}\t${KIND_NAMES[kind]}\t${hex32(transactionId)}\t` +
        `(${before.index},${before.source},${before.target}) -> ` +
        `(${after.index},${after.source},${after.target})\n`,
    );
  }
}

/**
 * Opens the data store and its append-only journal (`<db>.transitions.log`
 * or `--transactions-file`) under a `TransactionsDecorator`.
 */
function openTransactions(cli, commitMode, retention, { openNamedStorage, openJournal, err }) {
  const dataLinks = openNamedStorage(cli.db, cli);
  const journal = openJournal(cli.db, cli.transactionsFile);
  return new TransactionsDecorator(dataLinks, journal, retention, commitMode, cli.trace, {
    stderr: err,
  });
}

/** `--transactions`, `--log` and the other transactions options without version control. */
export async function runWithTransactions(cli, commitMode, retention, context) {
  const tx = openTransactions(cli, commitMode, retention, context);
  if (cli.showLog) {
    printLog(tx, context.out);
    tx.save();
    return;
  }
  await context.finish(cli, tx);
}

/** The `(name, sequence)` of a `--tag` spec: `NAME` or `NAME=POINT`. */
function resolveTag(vc, spec) {
  const first = spec.indexOf('=');
  if (first < 0) return [spec, null];
  // Preserve existing aliases before trying delimiters in the name:
  // alias=release=0 may refer to the complete existing tag release=0.
  for (let separator = first; separator >= 0; separator = spec.indexOf('=', separator + 1)) {
    const resolved = resolveSequence(vc, spec.slice(separator + 1));
    if (resolved !== null) return [spec.slice(0, separator), resolved];
  }
  throw new Error(`Unknown tag point '${spec.slice(first + 1)}'.`);
}

/** `--vc` and the checkout, branch, tag and listing options. */
export async function runWithVc(cli, commitMode, retention, context) {
  const { out, err } = context;
  const tx = openTransactions(cli, commitMode, retention, context);
  const vcLinks = context.openNamedStorage(
    cli.vcFile ?? makeVersionControlDatabaseFilename(cli.db),
    cli,
  );
  const vc = new VersionControlDecorator(tx, vcLinks, cli.trace, { stderr: err });

  // 1) --checkout (resolves seq or tag).
  if (cli.checkout != null) {
    const seq = resolveSequence(vc, cli.checkout);
    if (seq === null) throw new Error(`Unknown checkout point '${cli.checkout}'.`);
    vc.checkout(seq);
    if (cli.trace) out.write(`Checked out seq ${seq} on branch '${vc.currentBranch}'.\n`);
  }

  // 2) --branch [--branch-from] (creates if missing, then switches).
  if (cli.branch != null) {
    const name = cli.branch;
    if (!vc.listBranches().some((info) => info.name === name)) {
      vc.branch(name, cli.branchFrom);
      if (cli.trace) out.write(`Created branch '${name}'.\n`);
    }
    vc.switchBranch(name);
    if (cli.trace) out.write(`Switched to branch '${name}'.\n`);
  }

  // 3) --tag.
  if (cli.tag != null) {
    const [name, seq] = resolveTag(vc, cli.tag);
    vc.tag(name, seq);
    if (cli.trace) out.write(`Tagged '${name}' at seq ${seq ?? vc.currentSequence()}.\n`);
  }

  // 4) --list-branches / --list-tags (terminal).
  if (cli.listBranches) {
    const current = vc.currentBranch;
    for (const info of vc.listBranches()) {
      const marker = info.name === current ? '*' : ' ';
      out.write(
        `${marker} ${info.name}\tparent=${info.parent ?? '-'}\tfork=${info.forkSeq}\thead=${info.head}\n`,
      );
    }
    vc.save();
    return;
  }
  if (cli.listTags) {
    for (const [name, seq] of vc.listTags()) out.write(`${name}\t${seq}\n`);
    vc.save();
    return;
  }

  // 5) --log.
  if (cli.showLog) {
    printLog(vc.transactions, out);
    vc.save();
    return;
  }
  await context.finish(cli, vc);
}
