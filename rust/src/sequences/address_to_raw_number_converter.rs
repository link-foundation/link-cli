use crate::hybrid_reference::external_reference;

#[derive(Clone, Copy, Debug, Default)]
pub struct AddressToRawNumberConverter;

impl AddressToRawNumberConverter {
    pub fn new() -> Self {
        Self
    }

    pub fn convert(&self, address: u32) -> u32 {
        external_reference(address)
    }
}
