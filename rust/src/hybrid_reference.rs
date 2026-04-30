//! `Platform.Data.Hybrid<uint>`-compatible reference encoding.

const EXTERNAL_ZERO: u32 = (u32::MAX / 2) + 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HybridReference {
    encoded: u32,
}

impl HybridReference {
    pub fn external(value: u32) -> Self {
        Self {
            encoded: if value == 0 {
                EXTERNAL_ZERO
            } else {
                0u32.wrapping_sub(value)
            },
        }
    }

    pub fn from_encoded(encoded: u32) -> Self {
        Self { encoded }
    }

    pub fn encoded(self) -> u32 {
        self.encoded
    }

    pub fn absolute_value(self) -> Option<u32> {
        if self.encoded == EXTERNAL_ZERO {
            Some(0)
        } else if self.encoded >= EXTERNAL_ZERO {
            Some(0u32.wrapping_sub(self.encoded))
        } else {
            None
        }
    }

    pub fn is_external(self) -> bool {
        self.absolute_value().is_some()
    }
}

/// Encodes an external reference the same way `Platform.Data.Hybrid<uint>` does.
pub fn external_reference(value: u32) -> u32 {
    HybridReference::external(value).encoded()
}

/// Decodes a `Platform.Data.Hybrid<uint>` external reference.
pub fn external_reference_value(value: u32) -> Option<u32> {
    HybridReference::from_encoded(value).absolute_value()
}
