use crate::hybrid_reference::external_reference_value;

#[derive(Clone, Copy, Debug, Default)]
pub struct RawNumberToAddressConverter;

impl RawNumberToAddressConverter {
    pub fn new() -> Self {
        Self
    }

    pub fn convert(&self, raw_number: u32) -> u32 {
        external_reference_value(raw_number).unwrap_or(raw_number)
    }
}
