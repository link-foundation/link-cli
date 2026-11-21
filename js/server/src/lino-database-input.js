// `--in`: imports a LiNo file into a store (rust/src/lino_database_input.rs).
// Each top-level link `(index: source target)` writes the link at its index
// or name; references are numbers, names (found or created as named points)
// or nested anonymous links, found or created by their source and target.
// The whole document is validated before anything is written.
import { readFileSync } from 'node:fs';
import { parseU32 } from './link.js';
import { isEmptyLino, parseLino, valuesCount } from './parser.js';
import { trim } from './text-storage-format.js';

const utf8 = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true });

/** Imports the LiNo file at `path` into `storage`, then saves it. */
export function importLinoFile(storage, path) {
  const failure = (cause) => new Error(`Failed to read LiNo input: ${path}`, { cause });
  let bytes;
  try {
    bytes = readFileSync(path);
  } catch (error) {
    throw failure(error);
  }
  let text;
  try {
    text = utf8.decode(bytes);
  } catch {
    // `fs::read_to_string` reports undecodable text as an I/O error.
    throw failure(new Error('stream did not contain valid UTF-8'));
  }
  importLinoText(storage, text);
  storage.save();
}

/** Imports LiNo text into `storage`, without saving. */
export function importLinoText(storage, linksNotation) {
  const links = parseLino(normalizeLinksNotation(linksNotation)).filter(
    (value) => !isEmptyLino(value),
  );
  for (const value of links) validateLink(value, false);
  for (const value of links) importLink(storage, value);
}

function pairValues(value) {
  if (valuesCount(value) !== 2) {
    throw new Error('LiNo import supports links with exactly two values');
  }
  return value.values;
}

function validateLink(value, allowAnonymousIndex) {
  for (const reference of pairValues(value)) {
    const count = valuesCount(reference);
    if (count === 2) {
      validateLink(reference, true);
    } else if (count !== 0) {
      throw new Error(
        'LiNo import references must be scalar values or nested links with exactly two values',
      );
    } else if (normalizeIdentifier(reference.id) === '') {
      throw new Error('LiNo import references must have a value');
    }
  }
  if (!allowAnonymousIndex && normalizeIdentifier(value.id) === '') {
    throw new Error('Top-level LiNo import links must have an index or name');
  }
}

function importLink(storage, value) {
  const [sourceReference, targetReference] = value.values;
  const source = resolveReference(storage, sourceReference);
  const target = resolveReference(storage, targetReference);
  const identifier = normalizeIdentifier(value.id);
  if (identifier === '') return storage.tryGetOrCreate(source, target);
  const index = resolveIndex(storage, identifier);
  updateLink(storage, index, source, target);
  return index;
}

function resolveReference(storage, reference) {
  if (valuesCount(reference) === 2) return importLink(storage, reference);
  const identifier = normalizeIdentifier(reference.id);
  const id = parseU32(identifier);
  return id !== null ? id : storage.getOrCreateNamed(identifier);
}

function resolveIndex(storage, identifier) {
  const id = parseU32(identifier);
  if (id === null || id === 0) return storage.getOrCreateNamed(identifier);
  if (!storage.exists(id)) storage.tryEnsureCreated(id);
  return id;
}

/** Writes `(source target)` at `index`, creating the link first when missing. */
export function updateLink(storage, index, source, target) {
  if (!storage.exists(index)) storage.tryEnsureCreated(index);
  const current = storage.getLink(index);
  if (current != null && current.source === source && current.target === target) return;
  storage.update(index, source, target);
}

function normalizeIdentifier(identifier) {
  const normalized = trim(identifier ?? '').replace(/:+$/, '');
  if (normalized.length >= 2 && normalized.startsWith("'") && normalized.endsWith("'")) {
    return normalized.slice(1, -1).replaceAll("\\'", "'");
  }
  if (normalized.length >= 2 && normalized.startsWith('"') && normalized.endsWith('"')) {
    return normalized.slice(1, -1).replaceAll('\\"', '"');
  }
  return normalized;
}

// Rust `str::lines`, each line trimmed, blank lines dropped.
function normalizeLinksNotation(linksNotation) {
  return linksNotation
    .split('\n')
    .map((line) => trim(line))
    .filter((line) => line !== '')
    .join('\n');
}
