//! Binary links notation: a self-delimiting packet of links.
//!
//! A packet stores links, each a tuple of one or more references, at
//! implicit consecutive addresses. It knows nothing about LiNo; the LiNo
//! protocol ([`crate::protocol::encode_document`]) and the store archive
//! ([`crate::protocol::archive`]) are two uses of it.
//!
//! ```text
//! byte 0     0x10 | flags      high nibble 1 = format version 1
//!                              bit 0     external references (Hybrid encoding)
//!                              bit 1     explicit layout
//!                              bits 2-3  log2 of the width (compact layout only)
//!
//! compact layout (bit 1 clear): one section of doublets right after the markers
//! LEB128     N                 number of links, each `source target`
//!
//! explicit layout (bit 1 set):
//! LEB128     S                 number of sections, then S section headers:
//! LEB128     shape             bits 0-1  log2 of the reference width in bytes
//!                              bit 2     a gap follows
//!                              bit 3     variable arity (else every link
//!                                        holds exactly min_arity references)
//!                              bits 4+   min_arity, at least 1
//! LEB128     gap               addresses skipped before the section (if bit 2)
//! LEB128     extra_arity       0 = no maximum, else max - min (if bit 3)
//! LEB128     count             number of links in the section
//!
//! links, section by section; a link in a variable-arity section starts
//! with LEB128 (length - min_arity); every reference is `width` bytes,
//! little-endian
//! ```
//!
//! Address `0` is null. The first section starts at `1 + gap` and every
//! other section at `previous end + gap`, so gaps leave holes. The compact
//! layout is exactly one section with gap 5 (the LiNo marker points
//! `1..=5`), arity 2 and the header width.
//!
//! Each section has its own width: the narrowest of 1, 2, 4 and 8 bytes
//! that holds every reference in it, so links that only refer to small
//! addresses stay small wherever they live. [`LinksPacket::pack`] chooses
//! the sections.
//!
//! With external references enabled the top bit of a reference marks it as
//! external, exactly like `Platform.Data.Hybrid<T>`: value `v ≥ 1` is stored as
//! the two's-complement negation `2^bits - v` and value `0` as `2^(bits-1)`.
//! That halves the internal range of every width (`0..128` for 8-bit, …).

use super::error::{ProtocolError, ProtocolResult};
use std::fmt;
use std::io::{self, Read, Write};
use std::str::FromStr;

/// The high nibble of the header byte. Text messages never start with a byte
/// in `0x10..=0x1F`, so the header doubles as a protocol detector.
pub const BINARY_VERSION_1: u8 = 0x10;

const FLAG_EXTERNAL_REFERENCES: u8 = 0b0001;
const FLAG_EXPLICIT_LAYOUT: u8 = 0b0010;
const WIDTH_SHIFT: u8 = 2;
const WIDTH_BITS: u8 = 0b1100;

const SHAPE_WIDTH_BITS: u64 = 0b0011;
const SHAPE_HAS_GAP: u64 = 0b0100;
const SHAPE_VARIABLE_ARITY: u64 = 0b1000;
const SHAPE_MIN_ARITY_SHIFT: u32 = 4;

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
/// Address of the first link after the marker points.
pub const FIRST_LINK_ADDRESS: u64 = 6;

/// The addresses the compact layout skips: the marker points.
const COMPACT_GAP: u64 = FIRST_LINK_ADDRESS - 1;

/// The reference widths, in bytes, that a packet may use.
pub const WIDTHS: [u8; 4] = [1, 2, 4, 8];

/// One reference inside a packet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Reference {
    /// A link address (`0` is null).
    Internal(u64),
    /// An external value, e.g. a number or a Unicode code point.
    External(u64),
}

impl Reference {
    /// The null reference.
    pub const NULL: Reference = Reference::Internal(NULL);
}

/// How many references the links of a section hold: `min..=max`, where
/// `max = None` means no upper bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ArityRange {
    /// Fewest references in a link, at least 1.
    pub min: u64,
    /// Most references in a link, `None` for no limit.
    pub max: Option<u64>,
}

impl ArityRange {
    /// Links of exactly two references.
    pub const DOUBLETS: ArityRange = ArityRange::exactly(2);

    /// Links of exactly `arity` references.
    pub const fn exactly(arity: u64) -> Self {
        Self {
            min: arity,
            max: Some(arity),
        }
    }

    /// Links of at least `min` references.
    pub const fn at_least(min: u64) -> Self {
        Self { min, max: None }
    }

    /// Links of `min..=max` references.
    pub const fn between(min: u64, max: u64) -> Self {
        Self {
            min,
            max: Some(max),
        }
    }

    /// True when a link of `length` references fits the range.
    pub fn contains(&self, length: u64) -> bool {
        length >= self.min && self.max.is_none_or(|max| length <= max)
    }

    /// True when every link has the same number of references, so links
    /// need no length prefix.
    pub fn is_fixed(&self) -> bool {
        self.max == Some(self.min)
    }

    fn validate(&self) -> Result<(), String> {
        if self.min == 0 {
            return Err("arity must be at least 1".into());
        }
        if self.min > u64::MAX >> SHAPE_MIN_ARITY_SHIFT {
            return Err(format!("arity {} is too large", self.min));
        }
        if self.max.is_some_and(|max| max < self.min) {
            return Err(format!("arity range {self} is empty"));
        }
        Ok(())
    }

    /// `0` for no maximum, otherwise `max - min`; only for variable arities.
    fn extra(&self) -> u64 {
        self.max.map_or(0, |max| max - self.min)
    }

    fn from_shape(min: u64, extra: Option<u64>) -> ProtocolResult<Self> {
        let range = match extra {
            None => Self::exactly(min),
            Some(0) => Self::at_least(min),
            Some(extra) => Self::between(
                min,
                min.checked_add(extra)
                    .ok_or_else(|| ProtocolError::malformed("arity range overflows 64 bits"))?,
            ),
        };
        range.validate().map_err(ProtocolError::malformed)?;
        Ok(range)
    }
}

impl Default for ArityRange {
    fn default() -> Self {
        Self::DOUBLETS
    }
}

impl fmt::Display for ArityRange {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.max {
            Some(max) if max == self.min => write!(formatter, "{max}"),
            Some(max) => write!(formatter, "{}..{max}", self.min),
            None => write!(formatter, "{}..", self.min),
        }
    }
}

impl FromStr for ArityRange {
    type Err = String;

    /// Parses `n`, `min..max` (inclusive) or `min..` (no maximum).
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let number = |part: &str| {
            part.parse::<u64>()
                .map_err(|_| format!("invalid arity '{text}': expected n, min..max or min.."))
        };
        let range = match text.split_once("..") {
            None => Self::exactly(number(text)?),
            Some((min, "")) => Self::at_least(number(min)?),
            Some((min, max)) => Self::between(number(min)?, number(max)?),
        };
        range.validate()?;
        Ok(range)
    }
}

/// Safety limits applied while decoding untrusted input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodeLimits {
    /// Maximum number of links in a packet.
    pub max_links: u64,
    /// Maximum number of references in all links of a packet.
    pub max_references: u64,
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
            max_references: 1 << 24,
            max_nodes: 1 << 22,
            max_depth: 1024,
            max_text_bytes: 64 << 20,
        }
    }
}

impl DecodeLimits {
    /// Limits for trusted input such as a store archive: only the address
    /// space bounds the packet.
    pub fn unlimited() -> Self {
        Self {
            max_links: u64::MAX,
            max_references: u64::MAX,
            max_nodes: usize::MAX,
            max_depth: usize::MAX,
            max_text_bytes: usize::MAX,
        }
    }
}

/// A run of links at consecutive addresses sharing an arity range and a
/// reference width.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    /// Addresses skipped before the first link of the section.
    pub gap: u64,
    /// The number of references each link may hold.
    pub arity: ArityRange,
    /// Bytes per reference: 1, 2, 4 or 8.
    pub width: u8,
    /// The links, in address order.
    pub links: Vec<Vec<Reference>>,
}

/// A decoded binary links packet.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LinksPacket {
    /// Header bit 0: references may be external (Hybrid encoding).
    pub external_references: bool,
    /// The links, section by section.
    pub sections: Vec<Section>,
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

fn width_from_code(code: u64) -> ProtocolResult<u8> {
    usize::try_from(code)
        .ok()
        .and_then(|code| WIDTHS.get(code).copied())
        .ok_or_else(|| ProtocolError::malformed(format!("invalid width code {code}")))
}

/// The narrowest width able to hold `reference`.
pub fn reference_width(reference: Reference, external_references: bool) -> ProtocolResult<u8> {
    match reference {
        Reference::Internal(address) => {
            if address > internal_capacity(8, external_references) {
                return Err(ProtocolError::Unencodable(format!(
                    "address {address} exceeds the internal range"
                )));
            }
            Ok(address_tier(address, external_references))
        }
        Reference::External(value) => {
            if !external_references {
                return Err(ProtocolError::Unencodable(
                    "external reference in a packet without external references".into(),
                ));
            }
            WIDTHS
                .into_iter()
                .find(|&width| external_capacity(width) >= value)
                .ok_or_else(|| {
                    ProtocolError::Unencodable(format!("external value {value} exceeds 63 bits"))
                })
        }
    }
}

impl Section {
    fn write_header(&self, out: &mut Vec<u8>) -> ProtocolResult<()> {
        let mut shape =
            u64::from(width_code(self.width)?) | (self.arity.min << SHAPE_MIN_ARITY_SHIFT);
        if self.gap != 0 {
            shape |= SHAPE_HAS_GAP;
        }
        if !self.arity.is_fixed() {
            shape |= SHAPE_VARIABLE_ARITY;
        }
        write_leb128(out, shape);
        if self.gap != 0 {
            write_leb128(out, self.gap);
        }
        if !self.arity.is_fixed() {
            write_leb128(out, self.arity.extra());
        }
        write_leb128(out, self.links.len() as u64);
        Ok(())
    }

    /// Reads a section header, returning the still empty section and its
    /// link count.
    fn read_header(reader: &mut dyn Read) -> ProtocolResult<(Self, u64)> {
        let shape = read_leb128(reader)?;
        let width = width_from_code(shape & SHAPE_WIDTH_BITS)?;
        let gap = if shape & SHAPE_HAS_GAP != 0 {
            read_leb128(reader)?
        } else {
            0
        };
        let extra = if shape & SHAPE_VARIABLE_ARITY != 0 {
            Some(read_leb128(reader)?)
        } else {
            None
        };
        let arity = ArityRange::from_shape(shape >> SHAPE_MIN_ARITY_SHIFT, extra)?;
        let count = read_leb128(reader)?;
        let section = Section {
            gap,
            arity,
            width,
            links: Vec::new(),
        };
        Ok((section, count))
    }

    fn validate(&self) -> ProtocolResult<()> {
        width_code(self.width)?;
        self.arity.validate().map_err(ProtocolError::Unencodable)?;
        for link in &self.links {
            if !self.arity.contains(link.len() as u64) {
                return Err(ProtocolError::Unencodable(format!(
                    "a link of {} references in a section of arity {}",
                    link.len(),
                    self.arity
                )));
            }
        }
        Ok(())
    }
}

impl LinksPacket {
    /// An empty packet.
    pub fn new(external_references: bool) -> Self {
        Self {
            external_references,
            sections: Vec::new(),
        }
    }

    /// Lays out `links` — `(address, references)` in ascending address
    /// order — in as few bytes as the format allows.
    ///
    /// Holes between addresses start new sections. Without `packed_widths`
    /// every section uses the width of the widest reference, so all
    /// references have the same size. With it each section gets the
    /// narrowest width its links need and sections split wherever that saves
    /// bytes; the result is never larger than the uniform one.
    pub fn pack(
        external_references: bool,
        links: &[(u64, Vec<Reference>)],
        packed_widths: bool,
    ) -> ProtocolResult<Self> {
        let planner = SectionPlanner::new(external_references, links)?;
        let uniform_layout = planner.plan(false);
        let uniform = Self::lay_out(external_references, links, &uniform_layout);
        if !packed_widths {
            return Ok(uniform);
        }
        let packed_layout = planner.plan(true);
        if packed_layout == uniform_layout {
            return Ok(uniform);
        }
        let packed = Self::lay_out(external_references, links, &packed_layout);
        Ok(if packed.to_bytes()?.len() < uniform.to_bytes()?.len() {
            packed
        } else {
            uniform
        })
    }

    /// Splits `links` into sections of `(link count, width)`.
    fn lay_out(
        external_references: bool,
        links: &[(u64, Vec<Reference>)],
        layout: &[(usize, u8)],
    ) -> Self {
        let mut sections = Vec::with_capacity(layout.len());
        let mut next_address = 1u64;
        let mut remaining = links;
        for &(count, width) in layout {
            let (members, rest) = remaining.split_at(count);
            remaining = rest;
            let start = members[0].0;
            let (shortest, longest) = members
                .iter()
                .map(|(_, link)| link.len() as u64)
                .fold((u64::MAX, 0), |(shortest, longest), length| {
                    (shortest.min(length), longest.max(length))
                });
            sections.push(Section {
                gap: start - next_address,
                arity: ArityRange::between(shortest, longest),
                width,
                links: members.iter().map(|(_, link)| link.clone()).collect(),
            });
            next_address = start + count as u64;
        }
        Self {
            external_references,
            sections,
        }
    }

    /// Every link with its address, in address order.
    pub fn links(&self) -> impl Iterator<Item = (u64, &[Reference])> + '_ {
        let mut next_address = 1u64;
        self.sections.iter().flat_map(move |section| {
            let start = next_address.saturating_add(section.gap);
            next_address = start.saturating_add(section.links.len() as u64);
            section
                .links
                .iter()
                .enumerate()
                .map(move |(index, link)| (start + index as u64, link.as_slice()))
        })
    }

    /// The number of links in the packet.
    pub fn link_count(&self) -> u64 {
        self.sections
            .iter()
            .map(|section| section.links.len() as u64)
            .sum()
    }

    fn is_compact(&self) -> bool {
        match self.sections.as_slice() {
            [] => true,
            [only] => {
                only.gap == COMPACT_GAP
                    && only.arity == ArityRange::DOUBLETS
                    && !only.links.is_empty()
            }
            _ => false,
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
        let mut header = BINARY_VERSION_1;
        if self.external_references {
            header |= FLAG_EXTERNAL_REFERENCES;
        }
        let mut out = Vec::new();
        let mut next_address = 1u64;
        for section in &self.sections {
            section.validate()?;
            next_address = next_address
                .checked_add(section.gap)
                .and_then(|start| start.checked_add(section.links.len() as u64))
                .ok_or_else(|| ProtocolError::Unencodable("addresses overflow 64 bits".into()))?;
        }
        if self.is_compact() {
            let width = self.sections.first().map_or(1, |section| section.width);
            header |= width_code(width)? << WIDTH_SHIFT;
            out.push(header);
            write_leb128(&mut out, self.link_count());
        } else {
            out.push(header | FLAG_EXPLICIT_LAYOUT);
            write_leb128(&mut out, self.sections.len() as u64);
            for section in &self.sections {
                section.write_header(&mut out)?;
            }
        }
        for section in &self.sections {
            for link in &section.links {
                if !section.arity.is_fixed() {
                    write_leb128(&mut out, link.len() as u64 - section.arity.min);
                }
                for &reference in link {
                    write_raw(
                        &mut out,
                        self.raw_reference(reference, section.width)?,
                        section.width,
                    );
                }
            }
        }
        writer.write_all(&out)?;
        Ok(())
    }

    fn raw_reference(&self, reference: Reference, width: u8) -> ProtocolResult<u64> {
        if reference_width(reference, self.external_references)? > width {
            return Err(ProtocolError::Unencodable(format!(
                "{reference:?} does not fit {width} byte(s)"
            )));
        }
        Ok(match reference {
            Reference::Internal(address) => address,
            Reference::External(value) => encode_external(value, width),
        })
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
        let mut packet = LinksPacket::new(header & FLAG_EXTERNAL_REFERENCES != 0);
        let mut counts = Vec::new();
        if header & FLAG_EXPLICIT_LAYOUT == 0 {
            let width = width_from_code(u64::from((header & WIDTH_BITS) >> WIDTH_SHIFT))?;
            let count = read_leb128(reader)?;
            if count > 0 {
                packet.sections.push(Section {
                    gap: COMPACT_GAP,
                    arity: ArityRange::DOUBLETS,
                    width,
                    links: Vec::new(),
                });
                counts.push(count);
            }
        } else {
            if header & WIDTH_BITS != 0 {
                return Err(ProtocolError::malformed(
                    "the explicit layout keeps the header width bits clear",
                ));
            }
            let section_count = read_leb128(reader)?;
            if section_count > limits.max_links {
                return Err(Self::too_many_links(limits));
            }
            let mut next_address = 1u64;
            for _ in 0..section_count {
                let (section, count) = Section::read_header(reader)?;
                next_address = next_address
                    .checked_add(section.gap)
                    .and_then(|start| start.checked_add(count))
                    .ok_or_else(|| ProtocolError::malformed("addresses overflow 64 bits"))?;
                packet.sections.push(section);
                counts.push(count);
            }
        }
        counts
            .iter()
            .try_fold(0u64, |total, &count| total.checked_add(count))
            .filter(|&total| total <= limits.max_links)
            .ok_or_else(|| Self::too_many_links(limits))?;
        let mut references_left = limits.max_references;
        let external_references = packet.external_references;
        for (section, count) in packet.sections.iter_mut().zip(counts) {
            section.links.reserve(count.min(4096) as usize);
            for _ in 0..count {
                let length = if section.arity.is_fixed() {
                    section.arity.min
                } else {
                    let length = read_leb128(reader)?
                        .checked_add(section.arity.min)
                        .filter(|&length| section.arity.contains(length))
                        .ok_or_else(|| {
                            ProtocolError::malformed(format!(
                                "link length outside the section arity {}",
                                section.arity
                            ))
                        })?;
                    length
                };
                references_left = references_left.checked_sub(length).ok_or_else(|| {
                    ProtocolError::LimitExceeded(format!(
                        "references exceed the limit of {}",
                        limits.max_references
                    ))
                })?;
                let mut link = Vec::with_capacity(length.min(4096) as usize);
                for _ in 0..length {
                    let raw = read_raw(reader, section.width)?;
                    link.push(
                        match decode_external(raw, section.width).filter(|_| external_references) {
                            Some(value) => Reference::External(value),
                            None => Reference::Internal(raw),
                        },
                    );
                }
                section.links.push(link);
            }
        }
        Ok(Some(packet))
    }

    fn too_many_links(limits: &DecodeLimits) -> ProtocolError {
        ProtocolError::LimitExceeded(format!(
            "packet declares more than {} links",
            limits.max_links
        ))
    }
}

/// Estimated bytes of a section header (shape and count), used to weigh a split.
const SECTION_HEADER_ESTIMATE: u64 = 2;
/// Estimated extra bytes of a variable-arity section: its `extra_arity`
/// header field, and the length prefix of each link.
const VARIABLE_ARITY_ESTIMATE: u64 = 1;
const UNREACHABLE: u64 = u64::MAX / 4;
/// Planner states: a width code (0..4) times fixed (0) or variable (1) arity.
const STATES: usize = WIDTHS.len() * 2;

/// Splits links into sections with a linear dynamic program.
///
/// After link `k`, `cost[s]` is the fewest estimated bytes for links
/// `0..=k` with link `k` in a section of state `s`. A link either continues
/// the section of the previous link (same state, no hole between them and,
/// for a fixed arity, the same length) or opens a new section after the
/// cheapest previous state, paying for a section header.
struct SectionPlanner<'a> {
    links: &'a [(u64, Vec<Reference>)],
    needs: Vec<u8>,
}

impl<'a> SectionPlanner<'a> {
    fn new(external_references: bool, links: &'a [(u64, Vec<Reference>)]) -> ProtocolResult<Self> {
        let mut needs = Vec::with_capacity(links.len());
        let mut previous_address = 0u64;
        for (address, link) in links {
            if *address <= previous_address {
                return Err(ProtocolError::Unencodable(format!(
                    "link addresses must ascend from 1, got {address} after {previous_address}"
                )));
            }
            if link.is_empty() {
                return Err(ProtocolError::Unencodable(format!(
                    "link {address} has no references"
                )));
            }
            previous_address = *address;
            let mut need = 1u8;
            for &reference in link {
                need = need.max(reference_width(reference, external_references)?);
            }
            needs.push(need);
        }
        Ok(Self { links, needs })
    }

    fn first_cheapest(costs: &[u64; STATES]) -> usize {
        let mut best = 0;
        for state in 1..STATES {
            if costs[state] < costs[best] {
                best = state;
            }
        }
        best
    }

    /// The sections as `(link count, width)` pairs; with `packed_widths`
    /// every width may be used, otherwise only the widest one needed.
    fn plan(&self, packed_widths: bool) -> Vec<(usize, u8)> {
        if self.links.is_empty() {
            return Vec::new();
        }
        let widest = self.needs.iter().copied().max().unwrap_or(1);
        let allowed_widths = WIDTHS.map(|width| packed_widths || width == widest);
        let mut opens_section = vec![0u8; self.links.len()];
        let mut previous_best = vec![0u8; self.links.len()];
        let mut costs = [UNREACHABLE; STATES];
        for (index, (address, link)) in self.links.iter().enumerate() {
            let best = Self::first_cheapest(&costs);
            previous_best[index] = best as u8;
            let cheapest_before = if index == 0 { 0 } else { costs[best] };
            let continues = index > 0 && self.links[index - 1].0 + 1 == *address;
            let same_length = index > 0 && self.links[index - 1].1.len() == link.len();
            let mut next = [UNREACHABLE; STATES];
            for state in 0..STATES {
                let width_index = state / 2;
                let variable = state % 2 == 1;
                let width = WIDTHS[width_index];
                if !allowed_widths[width_index] || width < self.needs[index] {
                    continue;
                }
                let variable_estimate = if variable { VARIABLE_ARITY_ESTIMATE } else { 0 };
                let body = link.len() as u64 * u64::from(width) + variable_estimate;
                let opening_cost = cheapest_before + SECTION_HEADER_ESTIMATE + variable_estimate;
                let continuing_cost = if continues && (variable || same_length) {
                    costs[state]
                } else {
                    UNREACHABLE
                };
                if continuing_cost <= opening_cost {
                    next[state] = continuing_cost + body;
                } else {
                    next[state] = opening_cost + body;
                    opens_section[index] |= 1 << state;
                }
            }
            costs = next;
        }
        let mut sections = Vec::new();
        let mut state = Self::first_cheapest(&costs);
        let mut end = self.links.len();
        for index in (0..self.links.len()).rev() {
            if opens_section[index] & (1 << state) != 0 {
                sections.push((end - index, WIDTHS[state / 2]));
                end = index;
                state = usize::from(previous_best[index]);
            }
        }
        sections.reverse();
        sections
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
