// Hybrid references (rust/src/hybrid_reference.rs): an external value `v` is
// stored as the wrapping negation `2^32 - v`, in the upper half of the u32
// range, with zero kept apart as the first external address `2^31`.

/** The encoding of the external value zero, `u32::MAX / 2 + 1`. */
export const EXTERNAL_ZERO = 0x80000000;

const RANGE = 0x100000000;

/** `external_reference`: the address encoding the external value `value`. */
export function externalReference(value) {
  return value === 0 ? EXTERNAL_ZERO : (RANGE - value) % RANGE;
}

/** `external_reference_value`: the value an address encodes, or `null` if it is internal. */
export function externalReferenceValue(encoded) {
  if (encoded === EXTERNAL_ZERO) return 0;
  return encoded > EXTERNAL_ZERO ? RANGE - encoded : null;
}
