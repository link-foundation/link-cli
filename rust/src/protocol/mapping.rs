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
//! Typed values and lists are encoded as a variable-length sequence
//! `[marker, elements…]` when the sequence section is enabled, and as the
//! doublet `(marker chain)` otherwise, where `chain` is the nil-terminated cons
//! list `(e1 (e2 (… (en 0))))`. A plain list in sequence mode has no marker.
//! Identical sub-links are emitted once and shared, because links are
//! content-addressed.

use super::error::{ProtocolError, ProtocolResult};
use super::packet::{
    external_capacity, DecodeLimits, LinksPacket, Reference, FIRST_LINK_ADDRESS, IDENTIFIED, LIST,
    NULL, NUMBER, ONE, STRING,
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
    /// Use the variable-length sequence section for lists, strings and links
    /// with ids instead of cons chains of doublets.
    pub sequences: bool,
    /// Let the reference width grow with the address instead of using the
    /// width of the largest address for every reference.
    pub progressive_widths: bool,
}

impl BinaryLinoOptions {
    /// Enables or disables external references.
    pub fn with_external_references(mut self, enabled: bool) -> Self {
        self.external_references = enabled;
        self
    }

    /// Enables or disables the sequence section.
    pub fn with_sequences(mut self, enabled: bool) -> Self {
        self.sequences = enabled;
        self
    }

    /// Enables or disables progressive reference widths.
    pub fn with_progressive_widths(mut self, enabled: bool) -> Self {
        self.progressive_widths = enabled;
        self
    }
}

/// Converts a document into a packet.
pub fn encode_document(
    document: &[LiNo<String>],
    options: BinaryLinoOptions,
) -> ProtocolResult<LinksPacket> {
    let mut encoder = Encoder::new(options);
    if !document.is_empty() {
        let items = document
            .iter()
            .map(|link| encoder.encode(link))
            .collect::<Vec<_>>();
        if options.sequences {
            encoder.sequences.push(items);
        } else {
            let chain = encoder.chain(&items);
            encoder.doublets.push((Node::Internal(LIST), chain));
        }
    }
    encoder.finish()
}

/// Converts a packet back into a document.
pub fn decode_document(
    packet: &LinksPacket,
    limits: &DecodeLimits,
) -> ProtocolResult<LinoDocument> {
    let Some(root) = packet.last_address() else {
        return Ok(Vec::new());
    };
    let decoder = Decoder::new(packet, limits);
    let root = Reference::Internal(root);
    let items = match decoder.view(root)? {
        View::Sequence(items) if !starts_with_marker(items) => items.to_vec(),
        View::Doublet(Reference::Internal(LIST), chain) => decoder.chain(chain)?,
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
    Doublet(usize),
    Sequence(usize),
}

struct Encoder {
    options: BinaryLinoOptions,
    doublets: Vec<(Node, Node)>,
    sequences: Vec<Vec<Node>>,
    doublet_index: HashMap<(Node, Node), usize>,
    sequence_index: HashMap<Vec<Node>, usize>,
    powers: Vec<Node>,
}

impl Encoder {
    fn new(options: BinaryLinoOptions) -> Self {
        Self {
            options,
            doublets: Vec::new(),
            sequences: Vec::new(),
            doublet_index: HashMap::new(),
            sequence_index: HashMap::new(),
            powers: vec![Node::Internal(ONE)],
        }
    }

    fn doublet(&mut self, source: Node, target: Node) -> Node {
        if let Some(&index) = self.doublet_index.get(&(source, target)) {
            return Node::Doublet(index);
        }
        let index = self.doublets.len();
        self.doublets.push((source, target));
        self.doublet_index.insert((source, target), index);
        Node::Doublet(index)
    }

    fn sequence(&mut self, items: Vec<Node>) -> Node {
        if let Some(&index) = self.sequence_index.get(&items) {
            return Node::Sequence(index);
        }
        let index = self.sequences.len();
        self.sequences.push(items.clone());
        self.sequence_index.insert(items, index);
        Node::Sequence(index)
    }

    /// A two-element link. Fixed doublets may only refer to fixed doublets,
    /// so a pair holding a sequence becomes a two-element sequence.
    fn pair(&mut self, first: Node, second: Node) -> Node {
        if matches!(first, Node::Sequence(_)) || matches!(second, Node::Sequence(_)) {
            self.sequence(vec![first, second])
        } else {
            self.doublet(first, second)
        }
    }

    fn chain(&mut self, items: &[Node]) -> Node {
        items
            .iter()
            .rev()
            .fold(Node::Internal(NULL), |tail, &head| self.doublet(head, tail))
    }

    fn typed(&mut self, marker: u64, elements: Vec<Node>) -> Node {
        if self.options.sequences {
            let mut items = Vec::with_capacity(elements.len() + 1);
            items.push(Node::Internal(marker));
            items.extend(elements);
            self.sequence(items)
        } else {
            let chain = self.chain(&elements);
            self.doublet(Node::Internal(marker), chain)
        }
    }

    fn power(&mut self, exponent: usize) -> Node {
        while self.powers.len() <= exponent {
            let previous = *self.powers.last().expect("powers start with One");
            let next = self.doublet(previous, previous);
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
            .fold(last, |sum, &power| self.doublet(power, sum))
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
            return self.doublet(Node::Internal(NUMBER), unary);
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
            LiNo::Link { id: None, values } => match values.as_slice() {
                [] => Node::Internal(NULL),
                [first, second] => {
                    let first = self.encode(first);
                    let second = self.encode(second);
                    self.pair(first, second)
                }
                values => {
                    let elements = values.iter().map(|value| self.encode(value)).collect();
                    if self.options.sequences {
                        self.sequence(elements)
                    } else {
                        self.typed(LIST, elements)
                    }
                }
            },
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
        let resolve = |node: Node| match node {
            Node::Internal(address) => Reference::Internal(address),
            Node::External(value) => Reference::External(value),
            Node::Doublet(index) => Reference::Internal(FIRST_LINK_ADDRESS + index as u64),
            Node::Sequence(index) => {
                Reference::Internal(FIRST_LINK_ADDRESS + doublet_count + index as u64)
            }
        };
        let mut packet = LinksPacket {
            external_references: self.options.external_references,
            sequences_section: self.options.sequences,
            min_width: 1,
            doublets: self
                .doublets
                .iter()
                .map(|&(source, target)| (resolve(source), resolve(target)))
                .collect(),
            sequences: self
                .sequences
                .iter()
                .map(|items| items.iter().map(|&item| resolve(item)).collect())
                .collect(),
        };
        packet.min_width = packet.required_min_width(!self.options.progressive_widths)?;
        Ok(packet)
    }
}

enum View<'a> {
    Null,
    Marker(u64),
    External(u64),
    Doublet(Reference, Reference),
    Sequence(&'a [Reference]),
}

fn starts_with_marker(items: &[Reference]) -> bool {
    matches!(items.first(), Some(Reference::Internal(address)) if (ONE..FIRST_LINK_ADDRESS).contains(address))
}

struct Decoder<'a> {
    packet: &'a LinksPacket,
    limits: &'a DecodeLimits,
    /// `unary[i]` is the number doublet `i` denotes, if it is a unary number.
    unary: Vec<Option<u64>>,
}

impl<'a> Decoder<'a> {
    fn new(packet: &'a LinksPacket, limits: &'a DecodeLimits) -> Self {
        // Links only refer backwards, so one forward pass evaluates every
        // unary number without recursion.
        let mut unary: Vec<Option<u64>> = Vec::with_capacity(packet.doublets.len());
        for &(source, target) in &packet.doublets {
            let value_of = |reference: Reference| match reference {
                Reference::Internal(NULL) => Some(0),
                Reference::Internal(ONE) => Some(1),
                Reference::Internal(address) if address >= FIRST_LINK_ADDRESS => unary
                    .get((address - FIRST_LINK_ADDRESS) as usize)
                    .copied()
                    .flatten(),
                _ => None,
            };
            let value = value_of(source)
                .zip(value_of(target))
                .and_then(|(source, target)| source.checked_add(target));
            unary.push(value);
        }
        Self {
            packet,
            limits,
            unary,
        }
    }

    fn view(&self, reference: Reference) -> ProtocolResult<View<'a>> {
        let address = match reference {
            Reference::External(value) => return Ok(View::External(value)),
            Reference::Internal(NULL) => return Ok(View::Null),
            Reference::Internal(address) if address < FIRST_LINK_ADDRESS => {
                return Ok(View::Marker(address))
            }
            Reference::Internal(address) => address - FIRST_LINK_ADDRESS,
        };
        let doublets = self.packet.doublets.len() as u64;
        if address < doublets {
            let (source, target) = self.packet.doublets[address as usize];
            Ok(View::Doublet(source, target))
        } else {
            self.packet
                .sequences
                .get((address - doublets) as usize)
                .map(|items| View::Sequence(items.as_slice()))
                .ok_or_else(|| ProtocolError::malformed(format!("dangling reference {address}")))
        }
    }

    fn number(&self, reference: Reference) -> ProtocolResult<u64> {
        match reference {
            Reference::External(value) => Ok(value),
            Reference::Internal(NULL) => Ok(0),
            Reference::Internal(ONE) => Ok(1),
            Reference::Internal(address) if address >= FIRST_LINK_ADDRESS => self
                .unary
                .get((address - FIRST_LINK_ADDRESS) as usize)
                .copied()
                .flatten()
                .ok_or_else(|| ProtocolError::malformed("expected a unary number")),
            _ => Err(ProtocolError::malformed("expected a unary number")),
        }
    }

    /// The elements of a typed value given in doublet form: a cons chain,
    /// optionally ending in a sequence holding the remaining elements.
    fn chain(&self, mut tail: Reference) -> ProtocolResult<Vec<Reference>> {
        let mut elements = Vec::new();
        loop {
            match self.view(tail)? {
                View::Null => return Ok(elements),
                View::Doublet(head, next) => {
                    if elements.len() >= self.limits.max_nodes {
                        return Err(ProtocolError::LimitExceeded("chain too long".into()));
                    }
                    elements.push(head);
                    tail = next;
                }
                View::Sequence(items) => {
                    elements.extend_from_slice(items);
                    return Ok(elements);
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
                let LiNo::Link { values, .. } = self.list(values, depth, budget)? else {
                    unreachable!("list always returns a link");
                };
                Ok(LiNo::Link {
                    id: Some(id),
                    values,
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
        let values = elements
            .iter()
            .map(|&element| self.decode(element, depth + 1, budget))
            .collect::<ProtocolResult<Vec<_>>>()?;
        Ok(LiNo::Link { id: None, values })
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
        match self.view(reference)? {
            View::Null => Ok(LiNo::Link {
                id: None,
                values: Vec::new(),
            }),
            View::External(value) => Ok(LiNo::Ref(value.to_string())),
            View::Marker(marker) => Err(ProtocolError::malformed(format!(
                "marker {marker} used as a value"
            ))),
            View::Doublet(Reference::Internal(NUMBER), value) => {
                self.typed(NUMBER, &[value], depth, budget)
            }
            View::Doublet(Reference::Internal(marker), chain)
                if (ONE..FIRST_LINK_ADDRESS).contains(&marker) =>
            {
                let elements = self.chain(chain)?;
                self.typed(marker, &elements, depth, budget)
            }
            View::Doublet(source, target) => self.list(&[source, target], depth, budget),
            View::Sequence(items) => match items.split_first() {
                Some((&Reference::Internal(marker), elements))
                    if (ONE..FIRST_LINK_ADDRESS).contains(&marker) =>
                {
                    self.typed(marker, elements, depth, budget)
                }
                _ => self.list(items, depth, budget),
            },
        }
    }
}
