#!/usr/bin/env node

/**
 * Print text we do not control without letting it speak for us.
 *
 * Changeset descriptions, merged changeset bodies and file contents are
 * written by contributors, and GitHub Actions parses workflow commands out of
 * whatever a step prints: the runner's legacy parser locates `##[` anywhere
 * in a physical line, and the registered commands include add-mask and
 * stop-commands (actions/runner#4692). A description containing
 * `##[error]cache poisoned` annotates the run, `##[add-mask]...` burns mask
 * slots, and `::stop-commands::pause-logging` silences command processing
 * for every step after it (issue #60).
 *
 * The fix is to bracket the text between two workflow commands:
 *
 *     ::stop-commands::<token>
 *     <text, printed verbatim>
 *     ::<token>::
 *
 * While stopped, the runner parses nothing, so the text renders exactly as
 * written. The token is fresh 128-bit randomness per call: a guessable or
 * reused token would let the very text being printed resume command
 * processing early. Outside GitHub Actions the text is printed plainly, so
 * local runs stay readable.
 *
 * The fixed, trusted wording around a value ("Merged changeset content:")
 * stays outside the bracket, on its own line.
 */

import { randomBytes } from 'crypto';

/**
 * Print untrusted text, disabling workflow-command parsing while it is on
 * the log.
 *
 * @param {string} text - Contributor-controlled text to print verbatim
 * @param {(line: string) => void} [log] - Output sink; pass console.error to
 *   print to stderr. Markers and text always share one stream so their
 *   order is the stream order.
 */
export function printUntrusted(text, log = console.log) {
  if (process.env.GITHUB_ACTIONS !== 'true') {
    log(text);
    return;
  }

  const token = randomBytes(16).toString('hex');
  log(`::stop-commands::${token}`);
  log(text);
  log(`::${token}::`);
}
