//! Lossless mapping between LiNo documents and [`LinksPacket`]s.
//!
//! The mapping only uses links, the way linksplatform represents data:
//!
//! - `()` is the null link `0`.
//! - A numeric reference `n` is `(Number unary(n))`, where `unary(0)` is null,
//!   `2^0` is the marker `One`, `2^k` is `(2^(k-1) 2^(k-1))` and other numbers
//!   are right-nested sums of powers of two from the highest bit down. With
//!   external references enabled, `n` is sent as an external reference instead.
//! - Any other reference is `(String code points…)`; every code point is a
//!   unary number, or an external reference when those are enabled.
//! - A link without an id and with exactly two values is a plain doublet.
//! - A link without an id and any other number of values is a list.
//! - A link with an id is `(Identified id values…)`.
//! - The document is a list of its top-level links, stored last (the root).
//!
//! A link with two values is always a doublet. A list or typed value with
//! any other number of values is a single link of that many references when
//! [`BinaryLinoOptions::arity`] allows it (`[marker, elements…]` for typed
//! values, the bare elements for lists), and otherwise the doublet
//! `(marker chain)`, where `chain` is the nil-terminated cons list
//! `(e1 (e2 (… (en 0))))`. Identical sub-links are emitted once and shared,
//! because links are content-addressed.
//!
//! Links are numbered so that every link only refers to earlier ones:
//! doublets of plain doublets first, then the rest in creation order.

use super::error::{ProtocolError, ProtocolResult};
use super::packet::{
    external_capacity, ArityRange, DecodeLimits, LinksPacket, Reference, FIRST_LINK_ADDRESS,
    IDENTIFIED, LIST, NULL, NUMBER, ONE, STRING,
};
use links_notation::LiNo;
use std::collections::HashMap;

/// A LiNo document: the list of top-level links of a message.
pub type LinoDocument = Vec<LiNo<String>>;

/// Optional features of the binary LiNo protocol.
///
/// Every feature is off by default; each one can be switched on
/// independently, like stacking a decorator.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct BinaryLinoOptions {
    /// Send numbers and code points as Hybrid external references instead of
    /// in-band unary links. Halves the internal address range of each width.
    pub external_references: bool,
    /// The link lengths the encoder may use. The default, exactly 2, sends
    /// only doublets; `2..3` adds triplets and `1..` any length. The range
    /// must include 2.
    pub arity: ArityRange,
    /// Give every section of the packet the narrowest width its links need
    /// instead of one width for the whole packet.
    pub packed_widths: bool,
}

impl BinaryLinoOptions {
    /// Enables or disables external references.
    pub fn with_external_references(mut self, enabled: bool) -> Self {
        self.external_references = enabled;
        self
    }

    /// Sets the link lengths the encoder may use.
    pub fn with_arity(mut self, arity: ArityRange) -> Self {
        self.arity = arity;
        self
    }

    /// Enables or disables packed widths.
    pub fn with_packed_widths(mut self, enabled: bool) -> Self {
        self.packed_widths = enabled;
        self
    }

    /// The options a peer most likely used to write `packet`, so a reply can
    /// be written in the same style.
    pub fn of_packet(packet: &LinksPacket) -> Self {
        let (shortest, longest) = packet
            .links()
            .map(|(_, link)| link.len() as u64)
            .fold((2, 2), |(shortest, longest), length| {
                (shortest.min(length), longest.max(length))
            });
        let mut widths = packet.sections.iter().map(|section| section.width);
        let first_width = widths.next();
        Self {
            external_references: packet.external_references,
            arity: ArityRange::between(shortest, longest),
            packed_widths: widths.any(|width| Some(width) != first_width),
        }
    }
}

/// Converts a document into a packet.
pub fn encode_document(
    document: &[LiNo<String>],
    options: BinaryLinoOptions,
) -> ProtocolResult<LinksPacket> {
    if !options.arity.contains(2) {
        return Err(ProtocolError::Unencodable(format!(
            "arity {} does not include doublets (2)",
            options.arity
        )));
    }
    let mut encoder = Encoder::new(options);
    if !document.is_empty() {
        let items = document
            .iter()
            .map(|link| encoder.encode(link))
            .collect::<Vec<_>>();
        encoder.list(items);
    }
    encoder.finish()
}

/// Converts a packet back into a document.
pub fn decode_document(
    packet: &LinksPacket,
    limits: &DecodeLimits,
) -> ProtocolResult<LinoDocument> {
    let decoder = Decoder::new(packet, limits)?;
    let Some(root) = decoder.links.len().checked_sub(1) else {
        return Ok(Vec::new());
    };
    let root = Reference::Internal(FIRST_LINK_ADDRESS + root as u64);
    let items = match decoder.view(root) {
        View::Link(&[Reference::Internal(LIST), chain]) => decoder.chain(chain)?,
        View::Link(items) if !starts_with_marker(items) => items.to_vec(),
        _ => return Err(ProtocolError::malformed("the root link is not a list")),
    };
    let mut budget = limits.max_nodes;
    items
        .into_iter()
        .map(|item| decoder.decode(item, 0, &mut budget))
        .collect()
}

/// Parses a canonical unsigned decimal number (no sign, no leading zeros).
pub(crate) fn canonical_number(text: &str) -> Option<u64> {
    let canonical = !text.is_empty()
        && text.bytes().all(|byte| byte.is_ascii_digit())
        && (text == "0" || !text.starts_with('0'));
    canonical.then(|| text.parse().ok()).flatten()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Node {
    Internal(u64),
    External(u64),
    /// A doublet of plain doublets, markers and externals.
    Doublet(usize),
    /// Any other link; numbered after every [`Node::Doublet`].
    Tuple(usize),
}

struct Encoder {
    options: BinaryLinoOptions,
    doublets: Vec<Vec<Node>>,
    tuples: Vec<Vec<Node>>,
    created: HashMap<Vec<Node>, Node>,
    powers: Vec<Node>,
}

impl Encoder {
    fn new(options: BinaryLinoOptions) -> Self {
        Self {
            options,
            doublets: Vec::new(),
            tuples: Vec::new(),
            created: HashMap::new(),
            powers: vec![Node::Internal(ONE)],
        }
    }

    fn link(&mut self, items: Vec<Node>) -> Node {
        if let Some(&node) = self.created.get(&items) {
            return node;
        }
        let refers_to_tuple = items.iter().any(|item| matches!(item, Node::Tuple(_)));
        let node = if items.len() == 2 && !refers_to_tuple {
            self.doublets.push(items.clone());
            Node::Doublet(self.doublets.len() - 1)
        } else {
            self.tuples.push(items.clone());
            Node::Tuple(self.tuples.len() - 1)
        };
        self.created.insert(items, node);
        node
    }

    fn pair(&mut self, first: Node, second: Node) -> Node {
        self.link(vec![first, second])
    }

    fn chain(&mut self, items: &[Node]) -> Node {
        items
            .iter()
            .rev()
            .fold(Node::Internal(NULL), |tail, &head| self.pair(head, tail))
    }

    /// One link of `items` when the arity allows it, except that two items
    /// are always a doublet.
    fn fits_one_link(&self, items: usize) -> bool {
        items != 2 && self.options.arity.contains(items as u64)
    }

    fn typed(&mut self, marker: u64, elements: Vec<Node>) -> Node {
        if self.fits_one_link(elements.len() + 1) {
            let mut items = Vec::with_capacity(elements.len() + 1);
            items.push(Node::Internal(marker));
            items.extend(elements);
            self.link(items)
        } else {
            let chain = self.chain(&elements);
            self.pair(Node::Internal(marker), chain)
        }
    }

    fn list(&mut self, elements: Vec<Node>) -> Node {
        match elements.as_slice() {
            [] => Node::Internal(NULL),
            &[first, second] => self.pair(first, second),
            _ if self.fits_one_link(elements.len()) => self.link(elements),
            _ => self.typed(LIST, elements),
        }
    }

    fn power(&mut self, exponent: usize) -> Node {
        while self.powers.len() <= exponent {
            let previous = *self.powers.last().expect("powers start with One");
            let next = self.pair(previous, previous);
            self.powers.push(next);
        }
        self.powers[exponent]
    }

    fn unary(&mut self, value: u64) -> Node {
        let bits = (0..64).rev().filter(|bit| value & (1u64 << bit) != 0);
        let powers = bits.map(|bit| self.power(bit)).collect::<Vec<_>>();
        let Some((&last, rest)) = powers.split_last() else {
            return Node::Internal(NULL);
        };
        rest.iter()
            .rev()
            .fold(last, |sum, &power| self.pair(power, sum))
    }

    fn scalar(&mut self, value: u64) -> Node {
        if self.options.external_references && value <= external_capacity(8) {
            Node::External(value)
        } else {
            self.unary(value)
        }
    }

    fn reference(&mut self, text: &str) -> Node {
        if let Some(value) = canonical_number(text) {
            if self.options.external_references && value <= external_capacity(8) {
                return Node::External(value);
            }
            let unary = self.unary(value);
            return self.pair(Node::Internal(NUMBER), unary);
        }
        let code_points = text
            .chars()
            .map(|character| self.scalar(u64::from(u32::from(character))))
            .collect();
        self.typed(STRING, code_points)
    }

    fn encode(&mut self, link: &LiNo<String>) -> Node {
        match link {
            LiNo::Ref(text) => self.reference(text),
            LiNo::Link { id: None, values } => {
                let elements = values.iter().map(|value| self.encode(value)).collect();
                self.list(elements)
            }
            LiNo::Link {
                id: Some(id),
                values,
            } => {
                let mut elements = Vec::with_capacity(values.len() + 1);
                elements.push(self.reference(id));
                elements.extend(values.iter().map(|value| self.encode(value)));
                self.typed(IDENTIFIED, elements)
            }
        }
    }

    fn finish(self) -> ProtocolResult<LinksPacket> {
        let doublet_count = self.doublets.len() as u64;
        let resolve = |node: &Node| match *node {
            Node::Internal(address) => Reference::Internal(address),
            Node::External(value) => Reference::External(value),
            Node::Doublet(index) => Reference::Internal(FIRST_LINK_ADDRESS + index as u64),
            Node::Tuple(index) => {
                Reference::Internal(FIRST_LINK_ADDRESS + doublet_count + index as u64)
            }
        };
        let links: Vec<(u64, Vec<Reference>)> = self
            .doublets
            .iter()
            .chain(&self.tuples)
            .enumerate()
            .map(|(index, items)| {
                (
                    FIRST_LINK_ADDRESS + index as u64,
                    items.iter().map(resolve).collect(),
                )
            })
            .collect();
        LinksPacket::pack(
            self.options.external_references,
            &links,
            self.options.packed_widths,
        )
    }
}

enum View<'a> {
    Null,
    Marker(u64),
    External(u64),
    Link(&'a [Reference]),
}

fn is_marker(reference: Reference) -> bool {
    matches!(reference, Reference::Internal(address) if (ONE..FIRST_LINK_ADDRESS).contains(&address))
}

fn starts_with_marker(items: &[Reference]) -> bool {
    items.first().copied().is_some_and(is_marker)
}

struct Decoder<'a> {
    limits: &'a DecodeLimits,
    /// `links[i]` is the link at address `FIRST_LINK_ADDRESS + i`.
    links: Vec<&'a [Reference]>,
    /// `unary[i]` is the number link `i` denotes, if it is a unary number.
    unary: Vec<Option<u64>>,
}

impl<'a> Decoder<'a> {
    fn new(packet: &'a LinksPacket, limits: &'a DecodeLimits) -> ProtocolResult<Self> {
        let mut links: Vec<&'a [Reference]> = Vec::new();
        let mut unary: Vec<Option<u64>> = Vec::new();
        for (address, link) in packet.links() {
            let expected = FIRST_LINK_ADDRESS + links.len() as u64;
            if address != expected {
                return Err(ProtocolError::malformed(format!(
                    "a LiNo packet stores its links contiguously from address \
                     {FIRST_LINK_ADDRESS}, found link {address} where {expected} belongs"
                )));
            }
            if let Some(&target) = link.iter().find(
                |&&reference| matches!(reference, Reference::Internal(target) if target >= address),
            ) {
                return Err(ProtocolError::malformed(format!(
                    "link {address} refers to {target:?}, which is not an earlier link"
                )));
            }
            // Links only refer backwards, so one forward pass evaluates every
            // unary number without recursion.
            let value_of = |reference: Reference| match reference {
                Reference::Internal(NULL) => Some(0),
                Reference::Internal(ONE) => Some(1),
                Reference::Internal(address) if address >= FIRST_LINK_ADDRESS => {
                    unary[(address - FIRST_LINK_ADDRESS) as usize]
                }
                _ => None,
            };
            let value = match *link {
                [source, target] => value_of(source)
                    .zip(value_of(target))
                    .and_then(|(source, target)| source.checked_add(target)),
                _ => None,
            };
            unary.push(value);
            links.push(link);
        }
        Ok(Self {
            limits,
            links,
            unary,
        })
    }

    fn view(&self, reference: Reference) -> View<'a> {
        match reference {
            Reference::External(value) => View::External(value),
            Reference::Internal(NULL) => View::Null,
            Reference::Internal(address) if address < FIRST_LINK_ADDRESS => View::Marker(address),
            Reference::Internal(address) => {
                View::Link(self.links[(address - FIRST_LINK_ADDRESS) as usize])
            }
        }
    }

    fn number(&self, reference: Reference) -> ProtocolResult<u64> {
        match reference {
            Reference::External(value) => Ok(value),
            Reference::Internal(NULL) => Ok(0),
            Reference::Internal(ONE) => Ok(1),
            Reference::Internal(address) if address >= FIRST_LINK_ADDRESS => self.unary
                [(address - FIRST_LINK_ADDRESS) as usize]
                .ok_or_else(|| ProtocolError::malformed("expected a unary number")),
            _ => Err(ProtocolError::malformed("expected a unary number")),
        }
    }

    /// The elements of the cons list `(e1 (e2 (… (en 0))))`.
    fn chain(&self, mut tail: Reference) -> ProtocolResult<Vec<Reference>> {
        let mut elements = Vec::new();
        loop {
            match self.view(tail) {
                View::Null => return Ok(elements),
                View::Link(&[head, next]) => {
                    if elements.len() >= self.limits.max_nodes {
                        return Err(ProtocolError::LimitExceeded("chain too long".into()));
                    }
                    elements.push(head);
                    tail = next;
                }
                _ => return Err(ProtocolError::malformed("broken element chain")),
            }
        }
    }

    fn typed(
        &self,
        marker: u64,
        elements: &[Reference],
        depth: usize,
        budget: &mut usize,
    ) -> ProtocolResult<LiNo<String>> {
        match marker {
            NUMBER => match elements {
                [value] => Ok(LiNo::Ref(self.number(*value)?.to_string())),
                _ => Err(ProtocolError::malformed("a number needs exactly one value")),
            },
            STRING => {
                let mut text = String::with_capacity(elements.len());
                for &element in elements {
                    let code_point = self.number(element)?;
                    let character = u32::try_from(code_point)
                        .ok()
                        .and_then(char::from_u32)
                        .ok_or_else(|| {
                            ProtocolError::malformed(format!("invalid code point {code_point}"))
                        })?;
                    text.push(character);
                }
                Ok(LiNo::Ref(text))
            }
            LIST => self.list(elements, depth, budget),
            IDENTIFIED => {
                let (&id, values) = elements
                    .split_first()
                    .ok_or_else(|| ProtocolError::malformed("an identified link needs an id"))?;
                let LiNo::Ref(id) = self.decode(id, depth + 1, budget)? else {
                    return Err(ProtocolError::malformed("a link id must be a reference"));
                };
                Ok(LiNo::Link {
                    id: Some(id),
                    values: self.values(values, depth, budget)?,
                })
            }
            _ => Err(ProtocolError::malformed(format!(
                "marker {marker} cannot start a typed value"
            ))),
        }
    }

    fn list(
        &self,
        elements: &[Reference],
        depth: usize,
        budget: &mut usize,
    ) -> ProtocolResult<LiNo<String>> {
        Ok(LiNo::Link {
            id: None,
            values: self.values(elements, depth, budget)?,
        })
    }

    fn values(
        &self,
        elements: &[Reference],
        depth: usize,
        budget: &mut usize,
    ) -> ProtocolResult<Vec<LiNo<String>>> {
        elements
            .iter()
            .map(|&element| self.decode(element, depth + 1, budget))
            .collect()
    }

    fn decode(
        &self,
        reference: Reference,
        depth: usize,
        budget: &mut usize,
    ) -> ProtocolResult<LiNo<String>> {
        if depth >= self.limits.max_depth {
            return Err(ProtocolError::LimitExceeded(format!(
                "nesting deeper than {}",
                self.limits.max_depth
            )));
        }
        *budget = budget
            .checked_sub(1)
            .ok_or_else(|| ProtocolError::LimitExceeded("too many LiNo nodes".into()))?;
        match self.view(reference) {
            View::Null => Ok(LiNo::Link {
                id: None,
                values: Vec::new(),
            }),
            View::External(value) => Ok(LiNo::Ref(value.to_string())),
            View::Marker(marker) => Err(ProtocolError::malformed(format!(
                "marker {marker} used as a value"
            ))),
            View::Link(&[Reference::Internal(NUMBER), value]) => {
                self.typed(NUMBER, &[value], depth, budget)
            }
            View::Link(&[Reference::Internal(marker), chain])
                if is_marker(Reference::Internal(marker)) =>
            {
                let elements = self.chain(chain)?;
                self.typed(marker, &elements, depth, budget)
            }
            View::Link(items) => match items.split_first() {
                Some((&Reference::Internal(marker), elements)) if starts_with_marker(items) => {
                    self.typed(marker, elements, depth, budget)
                }
                _ => self.list(items, depth, budget),
            },
        }
    }
}
