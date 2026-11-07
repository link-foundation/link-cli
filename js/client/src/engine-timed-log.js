// Optional commit log decorator that remembers when each revision was written, so
// a restore can name a time. See docs/ENGINE-BACKUP.md.
const DECIMAL = /^\d+$/;

// A revision written again after a crash lost it replaces what the lost one and
// its successors claimed.
const surviving = (timeline) => {
  const entries = [];
  for (const entry of JSON.parse(timeline.read())) {
    const fields = entry.split('|');
    if (fields.length !== 2 || !fields.every((field) => DECIMAL.test(field)))
      throw new TypeError('malformed timeline record');
    const parsed = { revision: BigInt(fields[0]), written: Number(fields[1]) };
    while (entries.length > 0 && entries.at(-1).revision >= parsed.revision) entries.pop();
    entries.push(parsed);
  }
  return entries;
};

/**
 * Keeps `revision|unix milliseconds` in a second log for every appended record.
 * Records are unchanged, and the record is written and flushed first, so a crash
 * can lose a timestamp but never leave one for a record that is not there. Time
 * never runs backwards: a clock that steps back repeats the latest time.
 */
export class TimedCommitLog {
  #inner;
  #timeline;
  #clock;
  #latest;
  constructor(inner, timeline, { clock = Date.now } = {}) {
    this.#inner = inner;
    this.#timeline = timeline;
    this.#clock = clock;
    this.#latest = surviving(timeline).at(-1)?.written ?? 0;
  }
  get supportsDurableFlush() {
    return (
      this.#inner.supportsDurableFlush === true && this.#timeline.supportsDurableFlush === true
    );
  }
  get supportsReplacement() {
    return this.#inner.supportsReplacement === true;
  }
  /** The timeline stays: a restore refuses the revisions compaction dropped. */
  replace(records) {
    if (!this.supportsReplacement) throw Error('selected log does not support journal replacement');
    return this.#inner.replace(records);
  }
  read() {
    return this.#inner.read();
  }
  append(record) {
    const fields = record.split('|');
    if (!fields[0].startsWith('engine') || !DECIMAL.test(fields[1] ?? ''))
      throw new TypeError('cannot timestamp a record without a revision');
    this.#inner.append(record);
    this.#latest = Math.max(this.#latest, this.#clock());
    this.#timeline.append(`${fields[1]}|${this.#latest}`);
  }
  flush() {
    this.#inner.flush();
    this.#timeline.flush();
  }
}

/**
 * The latest revision written by `time` (a Date or unix milliseconds), for
 * `WasmEngine.restoreBackup`. Refuses a time before the first timestamp, because
 * the state then is not known.
 */
export function revisionAt(timeline, time) {
  const limit = Number(time);
  const found = surviving(timeline).findLast((entry) => entry.written <= limit);
  if (found === undefined) throw new RangeError('no revision was written by that time');
  return found.revision;
}
