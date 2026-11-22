// The shared source-spine formatter of `--structure`: references are resolved
// in the original unwind order. Port of rust/src/structure_format.rs.
import { NotFoundError } from './errors.js';

export function formatStructure(context, startId, visited, get, exists, reference) {
  const frames = [];
  let id = startId;
  let terminal;
  for (;;) {
    const value = get(context, id);
    if (value == null) throw new NotFoundError(id);
    if (visited.has(id)) {
      terminal = reference(context, id);
      break;
    }
    visited.add(id);
    frames.push([id, value]);
    if (exists(context, value.source) && !visited.has(value.source)) {
      id = value.source;
    } else {
      terminal = reference(context, value.source);
      break;
    }
  }

  // Name resolution can fail or have decorator side effects. Preserve its
  // deepest-first target/index order before emitting root-first prefixes.
  const parts = [];
  for (const [frameId, value] of frames.reverse()) {
    const target = reference(context, value.target);
    const index = reference(context, value.index);
    visited.delete(frameId);
    parts.push([index, target]);
  }
  let output = '';
  for (let i = parts.length - 1; i >= 0; i--) output += `(${parts[i][0]}: `;
  output += terminal;
  for (const [, target] of parts) output += ` ${target})`;
  return output;
}
