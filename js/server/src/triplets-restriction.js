// A triplet `(index: type from to)` and the restriction `eachTriplet` matches
// triplets against. Port of rust/src/triplets/restriction.rs, with the
// `Triplet` and `TripletChange` values of rust/src/triplets/mod.rs and
// rust/src/triplets/query.rs.

/** A frozen triplet `{ index, typeId, from, to }`; `from` and `to` are `0` when empty. */
export function triplet(index, typeId, from, to) {
  return Object.freeze({ index, typeId, from, to });
}

/** The order of the derived `Ord` of the Rust `Triplet`: index, type, from, to. */
export const compareTriplets = (left, right) =>
  left.index - right.index ||
  left.typeId - right.typeId ||
  left.from - right.from ||
  left.to - right.to;

/**
 * One change a query made: `{ before, after }`, where a creation has a `null`
 * `before`, a deletion a `null` `after`, and a read the same triplet on both
 * sides.
 */
export function tripletChange(before, after) {
  return { before, after };
}

/**
 * A restriction of `eachTriplet`: `null` matches any value, `0` an empty
 * `from` or `to`.
 */
export class TripletQuery {
  constructor(index = null, typeId = null, from = null, to = null) {
    this.index = index;
    this.typeId = typeId;
    this.from = from;
    this.to = to;
    Object.freeze(this);
  }

  /** Every triplet. */
  static any() {
    return new TripletQuery();
  }

  static byIndex(index) {
    return new TripletQuery(index);
  }

  static byType(typeId) {
    return new TripletQuery(null, typeId);
  }

  static byFrom(from) {
    return new TripletQuery(null, null, from);
  }

  static byTo(to) {
    return new TripletQuery(null, null, null, to);
  }

  /** Whether `value` satisfies every part of the restriction. */
  matches(value) {
    return (
      (this.index == null || this.index === value.index) &&
      (this.typeId == null || this.typeId === value.typeId) &&
      (this.from == null || this.from === value.from) &&
      (this.to == null || this.to === value.to)
    );
  }
}
