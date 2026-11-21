// Simplifies the raw changes of one query to the net change of every link.
// Port of rust/src/changes_simplifier.rs (`ChangesSimplifier.cs` in C#).
//
// The store reports every step it takes -- a link is emptied to `(i: 0 0)`
// before it is deleted and created as `(i: 0 0)` before it is set -- and the
// null link `(0: 0 0)` stands for "no link". The steps of one address form a
// chain, so its net change is the `before` of its first step and the `after`
// of its last. A link created and deleted within the query is not reported.
// Changes are ordered by their `after` state; equal ones keep their order.
import { isNull } from './link.js';

/** `changes` is an array of `[before, after]` links (never null). */
export function simplifyChanges(changes) {
  const net = [];
  const positionOfAddress = new Map();
  for (const [before, after] of changes) {
    const address = before.index !== 0 ? before.index : after.index;
    if (address === 0) continue;
    const position = positionOfAddress.get(address);
    if (position === undefined) {
      positionOfAddress.set(address, net.length);
      net.push([before, after]);
    } else {
      net[position] = [net[position][0], after];
    }
  }
  return net
    .filter(([before, after]) => !(isNull(before) && isNull(after)))
    .map((change, order) => ({ change, order }))
    .sort(
      (left, right) =>
        left.change[1].index - right.change[1].index ||
        left.change[1].source - right.change[1].source ||
        left.change[1].target - right.change[1].target ||
        left.order - right.order,
    )
    .map(({ change }) => change);
}
