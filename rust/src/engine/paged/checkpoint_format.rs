//! Independent root decision, leaving the existing page format unchanged.
use super::format::{hash, invalid, put, u32_at, u64_at};
use crate::LinkError;

#[derive(Clone)]
pub(super) struct Decision {
    pub identity: [u8; 16],
    pub epoch: u64,
    pub flags: u32,
}
impl Decision {
    pub fn bank(&self) -> usize {
        (self.epoch % 2) as usize
    }
    pub fn bank_id(&self) -> [u8; 16] {
        let mut mask = [0; 16];
        mask[..8].copy_from_slice(&self.epoch.to_le_bytes());
        mask[8..].copy_from_slice(b"BANK0001");
        std::array::from_fn(|i| self.identity[i] ^ mask[i])
    }
    pub fn encode(&self) -> [u8; 128] {
        let mut bytes = [0; 128];
        bytes[..8].copy_from_slice(b"CLNKBK01");
        bytes[8..24].copy_from_slice(&self.identity);
        put(&mut bytes, 24, self.epoch);
        bytes[32..36].copy_from_slice(&(self.bank() as u32).to_le_bytes());
        bytes[36..40].copy_from_slice(&self.flags.to_le_bytes());
        bytes[40..56].copy_from_slice(&self.bank_id());
        let sum = hash(&bytes[..120], 0xcbf29ce484222325);
        put(&mut bytes, 120, sum);
        bytes
    }
    pub fn decode(bytes: &[u8], slot: usize, flags: u32) -> Result<Option<Self>, LinkError> {
        if bytes.len() != 128 || hash(&bytes[..120], 0xcbf29ce484222325) != u64_at(bytes, 120) {
            return Ok(None);
        }
        if &bytes[..8] != b"CLNKBK01" || bytes[56..120].iter().any(|b| *b != 0) {
            return Err(invalid("unsupported checkpoint root version or padding"));
        }
        let decision = Self {
            identity: bytes[8..24].try_into().expect("fixed root identity"),
            epoch: u64_at(bytes, 24),
            flags: u32_at(bytes, 36),
        };
        if decision.flags != flags
            || u32_at(bytes, 32) as usize != decision.bank()
            || decision.bank() != slot
            || bytes[40..56] != decision.bank_id()
        {
            return Err(invalid("checkpoint root flags, bank or epoch identity"));
        }
        Ok(Some(decision))
    }
}

pub(super) fn select(
    raw: &[Option<Vec<u8>>; 2],
    flags: u32,
) -> Result<Option<Decision>, LinkError> {
    let a = Decision::decode(raw[0].as_deref().unwrap_or_default(), 0, flags)?;
    let b = Decision::decode(raw[1].as_deref().unwrap_or_default(), 1, flags)?;
    match (a, b) {
        (None, None) => Ok(None),
        (Some(one), None) | (None, Some(one)) => Ok(Some(one)),
        (Some(a), Some(b)) => {
            if a.identity != b.identity || a.epoch.abs_diff(b.epoch) != 1 {
                return Err(invalid("conflicting checkpoint root decisions"));
            }
            Ok(Some(if a.epoch > b.epoch { a } else { b }))
        }
    }
}
