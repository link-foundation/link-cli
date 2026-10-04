//! The binary links packet: the wire format of the binary LiNo protocol.
//!
//! ```text
//! byte 0      0x10 | flags        high nibble 1 = format version 1
//!                                 bit 0  external references (Hybrid encoding)
//!                                 bit 1  sequence section present
//!                                 bits 2-3  log2 of the minimum width in bytes
//! LEB128      N                   number of fixed doublets
//! LEB128      M                   number of sequences (only when bit 1 is set)
//! N times     source target       one fixed doublet, refs `width(a)` bytes each
//! M times     size ref_1 … ref_n  one sequence, size and refs `width(a)` bytes each
//! ```
//!
//! Addresses are implicit: `0` is null, `1..=5` are the reserved marker points
//! (never transmitted), the fixed doublets occupy `6..6+N` and the sequences
//! follow them, so the first sequence is "last fixed link + 1".
//!
//! Every reference stored in the link at address `a` uses
//! `width(a) = max(min_width, tier(a))` bytes, little-endian, where `tier(a)`
//! is the smallest of 1, 2, 4 and 8 bytes able to hold `a`. Because a link may
//! only refer to links *before* it, `tier(a)` always fits every internal
//! reference it can contain. With `min_width` equal to the tier of the largest
//! address the whole packet uses one uniform width (`0..256` addresses use
//! 8-bit references, `256..65536` 16-bit, and so on); with a smaller
//! `min_width` the width grows progressively with the address, the way LZW
//! code widths grow with the dictionary.
//!
//! With external references enabled the top bit of a reference marks it as
//! external, exactly like `Platform.Data.Hybrid<T>`: value `v ≥ 1` is stored as
//! the two's-complement negation `2^bits - v` and value `0` as `2^(bits-1)`.
//! That halves the internal range of every width (`0..128` for 8-bit, …).

use super::error::{ProtocolError, ProtocolResult};
use std::io::{self, Read, Write};

/// The high nibble of the header byte. Text messages never start with a byte
/// in `0x10..=0x1F`, so the header doubles as a protocol detector.
pub const BINARY_VERSION_1: u8 = 0x10;

const FLAG_EXTERNAL_REFERENCES: u8 = 0b0001;
const FLAG_SEQUENCES: u8 = 0b0010;
const WIDTH_SHIFT: u8 = 2;

/// Null link address.
pub const NULL: u64 = 0;
/// Marker point `1`: the unary *one*; powers of two are `2^k = (2^(k-1) 2^(k-1))`.
pub const ONE: u64 = 1;
/// Marker point `2`: `(Number unary)` is a non-negative integer.
pub const NUMBER: u64 = 2;
/// Marker point `3`: `(String code points…)` is a Unicode string.
pub const STRING: u64 = 3;
/// Marker point `4`: `(List elements…)` is a list of links.
pub const LIST: u64 = 4;
/// Marker point `5`: `(Identified id values…)` is a link with an id.
pub const IDENTIFIED: u64 = 5;
/// Address of the first transmitted link.
pub const FIRST_LINK_ADDRESS: u64 = 6;

/// The reference widths, in bytes, that a packet may use.
pub const WIDTHS: [u8; 4] = [1, 2, 4, 8];

/// One reference inside a packet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Reference {
    /// An address inside the packet (`0` null, `1..=5` markers, then links).
    Internal(u64),
    /// An external value, e.g. a number or a Unicode code point.
    External(u64),
}

impl Reference {
    /// The null reference.
    pub const NULL: Reference = Reference::Internal(NULL);
}

/// Safety limits applied while decoding untrusted input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodeLimits {
    /// Maximum `N + M`.
    pub max_links: u64,
    /// Maximum total number of references inside all sequences.
    pub max_sequence_items: u64,
    /// Maximum number of LiNo nodes a packet may expand to.
    pub max_nodes: usize,
    /// Maximum LiNo nesting depth.
    pub max_depth: usize,
    /// Maximum size of a text message in bytes.
    pub max_text_bytes: usize,
}

impl Default for DecodeLimits {
    fn default() -> Self {
        Self {
            max_links: 1 << 22,
            max_sequence_items: 1 << 24,
            max_nodes: 1 << 22,
            max_depth: 1024,
            max_text_bytes: 64 << 20,
        }
    }
}

/// A decoded binary links packet.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LinksPacket {
    /// Header bit 0: references may be external (Hybrid encoding).
    pub external_references: bool,
    /// Header bit 1: the packet carries a sequence section (possibly empty).
    pub sequences_section: bool,
    /// Minimum reference width in bytes: 1, 2, 4 or 8.
    pub min_width: u8,
    /// Fixed doublets at addresses `6..6+N`.
    pub doublets: Vec<(Reference, Reference)>,
    /// Variable-length sequences at addresses `6+N..6+N+M`.
    pub sequences: Vec<Vec<Reference>>,
}

/// Largest internal address that fits in `width` bytes.
pub fn internal_capacity(width: u8, external_references: bool) -> u64 {
    let bits = u32::from(width) * 8 - u32::from(external_references);
    if bits >= 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    }
}

/// Largest external value that fits in `width` bytes.
pub fn external_capacity(width: u8) -> u64 {
    (1u64 << (u32::from(width) * 8 - 1)) - 1
}

/// Largest unsigned value (a sequence size) that fits in `width` bytes.
pub fn unsigned_capacity(width: u8) -> u64 {
    internal_capacity(width, false)
}

/// The narrowest width able to hold the internal address `address`.
pub fn address_tier(address: u64, external_references: bool) -> u8 {
    WIDTHS
        .into_iter()
        .find(|&width| internal_capacity(width, external_references) >= address)
        .unwrap_or(8)
}

fn width_mask(width: u8) -> u64 {
    internal_capacity(width, false)
}

/// Encodes an external value at `width` the way `Platform.Data.Hybrid<T>` does.
pub fn encode_external(value: u64, width: u8) -> u64 {
    if value == 0 {
        1u64 << (u32::from(width) * 8 - 1)
    } else {
        value.wrapping_neg() & width_mask(width)
    }
}

/// Decodes a raw `width`-byte value, returning `Some(value)` for externals.
pub fn decode_external(raw: u64, width: u8) -> Option<u64> {
    let external_zero = 1u64 << (u32::from(width) * 8 - 1);
    if raw == external_zero {
        Some(0)
    } else if raw > external_zero {
        Some(raw.wrapping_neg() & width_mask(width))
    } else {
        None
    }
}

fn width_code(width: u8) -> ProtocolResult<u8> {
    WIDTHS
        .iter()
        .position(|&candidate| candidate == width)
        .map(|code| code as u8)
        .ok_or_else(|| ProtocolError::Unencodable(format!("invalid width {width}")))
}

impl LinksPacket {
    /// Address of the fixed doublet with zero-based index `index`.
    pub fn doublet_address(&self, index: usize) -> u64 {
        FIRST_LINK_ADDRESS + index as u64
    }

    /// Address of the sequence with zero-based index `index`.
    pub fn sequence_address(&self, index: usize) -> u64 {
        FIRST_LINK_ADDRESS + self.doublets.len() as u64 + index as u64
    }

    /// The highest address in the packet, or `None` for an empty packet.
    pub fn last_address(&self) -> Option<u64> {
        let count = self.doublets.len() + self.sequences.len();
        (count > 0).then(|| FIRST_LINK_ADDRESS + count as u64 - 1)
    }

    /// Width used by every reference of the link at `address`.
    pub fn width_at(&self, address: u64) -> u8 {
        self.min_width
            .max(address_tier(address, self.external_references))
    }

    /// The smallest `min_width` able to encode the packet.
    ///
    /// With `uniform` set, the result is at least the tier of the last
    /// address, so every reference in the packet has the same width.
    pub fn required_min_width(&self, uniform: bool) -> ProtocolResult<u8> {
        let mut needed = 1u8;
        let mut note = |address: u64, need: u8| {
            if need > address_tier(address, self.external_references) {
                needed = needed.max(need);
            }
        };
        for (index, &(source, target)) in self.doublets.iter().enumerate() {
            let address = self.doublet_address(index);
            note(
                address,
                self.reference_need(source)?
                    .max(self.reference_need(target)?),
            );
        }
        for (index, items) in self.sequences.iter().enumerate() {
            let address = self.sequence_address(index);
            let size_need = WIDTHS
                .into_iter()
                .find(|&width| unsigned_capacity(width) >= items.len() as u64)
                .unwrap_or(8);
            let mut need = size_need;
            for &item in items {
                need = need.max(self.reference_need(item)?);
            }
            note(address, need);
        }
        if uniform {
            if let Some(last) = self.last_address() {
                needed = needed.max(address_tier(last, self.external_references));
            }
        }
        Ok(needed)
    }

    fn reference_need(&self, reference: Reference) -> ProtocolResult<u8> {
        match reference {
            Reference::Internal(_) => Ok(1),
            Reference::External(value) => {
                if !self.external_references {
                    return Err(ProtocolError::Unencodable(
                        "external reference in a packet without external references".into(),
                    ));
                }
                WIDTHS
                    .into_iter()
                    .find(|&width| external_capacity(width) >= value)
                    .ok_or_else(|| {
                        ProtocolError::Unencodable(format!(
                            "external value {value} exceeds 63 bits"
                        ))
                    })
            }
        }
    }

    fn header_byte(&self) -> ProtocolResult<u8> {
        let mut header = BINARY_VERSION_1 | (width_code(self.min_width)? << WIDTH_SHIFT);
        if self.external_references {
            header |= FLAG_EXTERNAL_REFERENCES;
        }
        if self.sequences_section {
            header |= FLAG_SEQUENCES;
        }
        Ok(header)
    }

    fn raw_reference(&self, reference: Reference, address: u64, width: u8) -> ProtocolResult<u64> {
        match reference {
            Reference::Internal(target) => {
                if target >= address {
                    return Err(ProtocolError::Unencodable(format!(
                        "link {address} refers forward to {target}"
                    )));
                }
                Ok(target)
            }
            Reference::External(value) => {
                if !self.external_references || value > external_capacity(width) {
                    return Err(ProtocolError::Unencodable(format!(
                        "external value {value} does not fit {width} byte(s) at link {address}"
                    )));
                }
                Ok(encode_external(value, width))
            }
        }
    }

    /// Serializes the packet.
    pub fn to_bytes(&self) -> ProtocolResult<Vec<u8>> {
        let mut bytes = Vec::new();
        self.write_to(&mut bytes)?;
        Ok(bytes)
    }

    /// Writes the packet to `writer`.
    pub fn write_to(&self, writer: &mut dyn Write) -> ProtocolResult<()> {
        if !self.sequences_section && !self.sequences.is_empty() {
            return Err(ProtocolError::Unencodable(
                "sequences in a packet without a sequence section".into(),
            ));
        }
        let mut out = vec![self.header_byte()?];
        write_leb128(&mut out, self.doublets.len() as u64);
        if self.sequences_section {
            write_leb128(&mut out, self.sequences.len() as u64);
        }
        for (index, &(source, target)) in self.doublets.iter().enumerate() {
            let address = self.doublet_address(index);
            let width = self.width_at(address);
            write_raw(&mut out, self.raw_reference(source, address, width)?, width);
            write_raw(&mut out, self.raw_reference(target, address, width)?, width);
        }
        for (index, items) in self.sequences.iter().enumerate() {
            let address = self.sequence_address(index);
            let width = self.width_at(address);
            let size = items.len() as u64;
            if size > unsigned_capacity(width) {
                return Err(ProtocolError::Unencodable(format!(
                    "sequence size {size} does not fit {width} byte(s) at link {address}"
                )));
            }
            write_raw(&mut out, size, width);
            for &item in items {
                write_raw(&mut out, self.raw_reference(item, address, width)?, width);
            }
        }
        writer.write_all(&out)?;
        Ok(())
    }

    /// Parses a complete packet; trailing bytes are an error.
    pub fn from_bytes(bytes: &[u8], limits: &DecodeLimits) -> ProtocolResult<Self> {
        let mut cursor = bytes;
        let packet = Self::read_from(&mut cursor, limits)?
            .ok_or_else(|| ProtocolError::malformed("empty input"))?;
        if !cursor.is_empty() {
            return Err(ProtocolError::malformed(format!(
                "{} trailing byte(s) after the packet",
                cursor.len()
            )));
        }
        Ok(packet)
    }

    /// Reads one packet from `reader`; `Ok(None)` on a clean end of stream.
    pub fn read_from(reader: &mut dyn Read, limits: &DecodeLimits) -> ProtocolResult<Option<Self>> {
        let mut header = [0u8; 1];
        if read_exact_or_eof(reader, &mut header)? {
            return Ok(None);
        }
        let header = header[0];
        if header & 0xF0 != BINARY_VERSION_1 {
            return Err(ProtocolError::malformed(format!(
                "unsupported binary header byte 0x{header:02X}"
            )));
        }
        let mut packet = LinksPacket {
            external_references: header & FLAG_EXTERNAL_REFERENCES != 0,
            sequences_section: header & FLAG_SEQUENCES != 0,
            min_width: WIDTHS[usize::from((header >> WIDTH_SHIFT) & 0b11)],
            ..LinksPacket::default()
        };
        let doublet_count = read_leb128(reader)?;
        let sequence_count = if packet.sequences_section {
            read_leb128(reader)?
        } else {
            0
        };
        let total = doublet_count
            .checked_add(sequence_count)
            .filter(|&total| total <= limits.max_links)
            .ok_or_else(|| {
                ProtocolError::LimitExceeded(format!(
                    "packet declares {doublet_count} + {sequence_count} links, limit is {}",
                    limits.max_links
                ))
            })?;
        packet.doublets.reserve(total.min(4096) as usize);
        for index in 0..doublet_count as usize {
            let address = packet.doublet_address(index);
            let width = packet.width_at(address);
            let source = packet.read_reference(reader, address, width)?;
            let target = packet.read_reference(reader, address, width)?;
            packet.doublets.push((source, target));
        }
        let mut items_left = limits.max_sequence_items;
        for index in 0..sequence_count as usize {
            let address = packet.sequence_address(index);
            let width = packet.width_at(address);
            let size = read_raw(reader, width)?;
            items_left = items_left.checked_sub(size).ok_or_else(|| {
                ProtocolError::LimitExceeded(format!(
                    "sequence items exceed the limit of {}",
                    limits.max_sequence_items
                ))
            })?;
            let mut items = Vec::with_capacity(size.min(4096) as usize);
            for _ in 0..size {
                items.push(packet.read_reference(reader, address, width)?);
            }
            packet.sequences.push(items);
        }
        Ok(Some(packet))
    }

    fn read_reference(
        &self,
        reader: &mut dyn Read,
        address: u64,
        width: u8,
    ) -> ProtocolResult<Reference> {
        let raw = read_raw(reader, width)?;
        if self.external_references {
            if let Some(value) = decode_external(raw, width) {
                return Ok(Reference::External(value));
            }
        }
        if raw >= address {
            return Err(ProtocolError::malformed(format!(
                "link {address} refers to {raw}, which is not an earlier link"
            )));
        }
        Ok(Reference::Internal(raw))
    }
}

fn write_raw(out: &mut Vec<u8>, value: u64, width: u8) {
    out.extend_from_slice(&value.to_le_bytes()[..usize::from(width)]);
}

fn read_raw(reader: &mut dyn Read, width: u8) -> ProtocolResult<u64> {
    let mut bytes = [0u8; 8];
    read_exact(reader, &mut bytes[..usize::from(width)])?;
    Ok(u64::from_le_bytes(bytes))
}

/// Appends `value` as unsigned LEB128.
pub fn write_leb128(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let byte = (value & 0x7F) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

/// Reads an unsigned LEB128 value of at most 64 bits.
pub fn read_leb128(reader: &mut dyn Read) -> ProtocolResult<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let mut byte = [0u8; 1];
        read_exact(reader, &mut byte)?;
        let payload = u64::from(byte[0] & 0x7F);
        if shift == 63 && payload > 1 {
            return Err(ProtocolError::malformed("LEB128 value overflows 64 bits"));
        }
        value |= payload << shift;
        if byte[0] & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(ProtocolError::malformed("LEB128 value overflows 64 bits"))
}

fn read_exact(reader: &mut dyn Read, buffer: &mut [u8]) -> ProtocolResult<()> {
    reader.read_exact(buffer).map_err(|error| {
        if error.kind() == io::ErrorKind::UnexpectedEof {
            ProtocolError::malformed("unexpected end of packet")
        } else {
            ProtocolError::Io(error)
        }
    })
}

/// Fills `buffer`, returning `true` if the stream ended before the first byte.
fn read_exact_or_eof(reader: &mut dyn Read, buffer: &mut [u8]) -> ProtocolResult<bool> {
    loop {
        match reader.read(&mut buffer[..1]) {
            Ok(0) => return Ok(true),
            Ok(_) => break,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.into()),
        }
    }
    read_exact(reader, &mut buffer[1..])?;
    Ok(false)
}
