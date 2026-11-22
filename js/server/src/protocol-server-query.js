// Checked request execution shared by the generic and session servers: ports
// rust/src/protocol/server_query.rs (`execute_request`,
// `execute_observed_request`, `execute_request_for`, `reference_name`).
// `query_document`, `admit_document` and `parse_document_with_limits` live in
// protocol-lino.js with the document model.
//
// Addresses are u32 numbers here, so `executeRequestFor` (Rust's request
// over any `QueryAddress`) is `executeRequest` without the change log.
import { NULL_LINK } from './link.js';
import { LinksOperation, parseChanges } from './protocol-links-operations.js';
import { alternateText } from './protocol-errors.js';
import { admitDocument, errorDocument, formatDocument, isRef, rustTrim } from './protocol-lino.js';
import { LinkError } from './errors.js';
import { QueryReplyBudget } from './query-options.js';

/**
 * `execute_request`: a non-empty document is a substitution query or an
 * operation; the empty document lists every link. Failures produce an
 * `(error: 'message')` reply.
 */
export function executeRequest(storage, processor, document) {
  return executeObservedRequest(storage, processor, document, []);
}

/** `execute_observed_request`: also records the net `[before, after]` changes in `changed`. */
export function executeObservedRequest(storage, processor, document, changed) {
  try {
    return tryExecuteRequest(storage, processor, document, changed);
  } catch (error) {
    return errorDocument(alternateText(error));
  }
}

/** `execute_request_for`: a request whose changes nobody observes. */
export function executeRequestFor(storage, processor, document) {
  return executeObservedRequest(storage, processor, document, []);
}

/** `reference_name`: the name of `id`, or its number. */
export function referenceName(storage, id) {
  return storage.getName(id) ?? String(id);
}

function tryExecuteRequest(storage, processor, document, changed) {
  const limits = storage.queryLimits();
  if (limits !== null) admitDocument(document, limits);
  // A canonical document does not retain quoted-reference kinds: the
  // `(query: text)` envelope carries the original query text.
  let original = null;
  const [first] = document;
  if (document.length === 1 && !isRef(first) && first.id === 'query') {
    if (first.values.length !== 1 || !isRef(first.values[0])) {
      throw new LinkError('expected (query: original-query-text)');
    }
    original = first.values[0];
  }
  if (original === null) {
    const operation = LinksOperation.fromDocument(document);
    if (operation !== null) {
      const reply = operation.execute(storage);
      if (['create', 'update', 'delete'].includes(operation.kind)) {
        changed.push(...parseChanges(reply));
      }
      return reply;
    }
  }
  if (document.length === 0 || (original !== null && rustTrim(original) === '')) {
    const budget = new QueryReplyBudget(storage.queryLimits());
    const links = [...storage.allLinks()];
    const checked = storage.queryLimits();
    if (checked !== null && links.length > checked.maxResults) {
      throw new LinkError('protocol result limit exceeded');
    }
    links.sort((a, b) => a.index - b.index);
    return links.map((value) => linkLino(storage, value, budget));
  }
  const text = original ?? formatDocument(document);
  const changes = processor.processQuery(storage, text);
  for (const [before, after] of changes) changed.push([before ?? NULL_LINK, after ?? NULL_LINK]);
  if (changes.length > 0) storage.save();
  const budget = new QueryReplyBudget(storage.queryLimits());
  return changes.map(([before, after]) => ({
    id: null,
    values: [changeSide(storage, before, budget), changeSide(storage, after, budget)],
  }));
}

/** `(index: source target)`, naming every reference that has a name. */
function linkLino(storage, value, budget) {
  const index = budget.reference(storage, value.index);
  const source = budget.reference(storage, value.source);
  const target = budget.reference(storage, value.target);
  return { id: index, values: [source, target] };
}

/** `()` for a missing side of a change, `((index: source target))` otherwise. */
function changeSide(storage, value, budget) {
  return { id: null, values: value == null ? [] : [linkLino(storage, value, budget)] };
}
