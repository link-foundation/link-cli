//! Interchangeable message protocols for LiNo documents.
//!
//! Both [`TextLinoProtocol`] and [`BinaryLinoProtocol`] implement
//! [`LinoProtocol`], so code written against the trait switches between them
//! by swapping one value. [`LinoConnection`] decorates any byte stream (a
//! `TcpStream`, a pipe, an in-memory buffer) with a protocol.

use super::error::{ProtocolError, ProtocolResult};
use links_notation::binary::packet::BINARY_VERSION_1;
use links_notation::binary::{
    decode_document, encode_document_with_limits, format_document, parse_document,
    BinaryLinoOptions, DecodeLimits, LinksPacket, LinoDocument,
};
use links_notation::LiNo;
use std::fmt::Debug;
use std::io::{BufRead, BufReader, Read, Write};

/// Reads and writes whole LiNo documents on a byte stream.
pub trait LinoProtocol: Debug + Send + Sync {
    /// Writes one message carrying `document`.
    fn write_document(
        &self,
        writer: &mut dyn Write,
        document: &[LiNo<String>],
    ) -> ProtocolResult<()>;

    /// Reads one message; `Ok(None)` when the stream ended cleanly.
    fn read_document(&self, reader: &mut dyn BufRead) -> ProtocolResult<Option<LinoDocument>>;
}

impl<P: LinoProtocol + ?Sized> LinoProtocol for Box<P> {
    fn write_document(
        &self,
        writer: &mut dyn Write,
        document: &[LiNo<String>],
    ) -> ProtocolResult<()> {
        (**self).write_document(writer, document)
    }

    fn read_document(&self, reader: &mut dyn BufRead) -> ProtocolResult<Option<LinoDocument>> {
        (**self).read_document(reader)
    }
}

/// The default limit of a text message: 64 MiB.
pub const DEFAULT_MAX_TEXT_BYTES: usize = 64 << 20;

/// Limits applied to incoming messages of either protocol.
///
/// The binary limits belong to links-notation; the text limit is link-cli's
/// own, because framing text messages is part of the transport, not of the
/// notation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProtocolLimits {
    /// Limits of [`BinaryLinoProtocol`] messages.
    pub binary: DecodeLimits,
    /// Maximum size of a [`TextLinoProtocol`] message in bytes.
    pub max_text_bytes: usize,
}

impl Default for ProtocolLimits {
    fn default() -> Self {
        Self {
            binary: DecodeLimits::default(),
            max_text_bytes: DEFAULT_MAX_TEXT_BYTES,
        }
    }
}

impl ProtocolLimits {
    /// Limits for trusted input: only the address space bounds a message.
    pub fn unlimited() -> Self {
        Self {
            binary: DecodeLimits::unlimited(),
            max_text_bytes: usize::MAX,
        }
    }
}

/// UTF-8 LiNo text, one message per block of lines ended by a line holding
/// only `.`. Lines starting with `.` get one extra `.` (SMTP dot-stuffing).
///
/// Lines may end with `\n` or `\r\n`; a carriage return right before a line
/// feed is dropped, so a quoted reference holding `\r\n` arrives as `\n`
/// (use [`BinaryLinoProtocol`] to carry such references exactly).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextLinoProtocol {
    /// Maximum size of a message read, in bytes.
    pub max_text_bytes: usize,
}

impl Default for TextLinoProtocol {
    fn default() -> Self {
        Self {
            max_text_bytes: DEFAULT_MAX_TEXT_BYTES,
        }
    }
}

impl TextLinoProtocol {
    /// A text protocol with default limits.
    pub fn new() -> Self {
        Self::default()
    }

    /// Frames already formatted LiNo text as one message.
    pub fn write_text(writer: &mut dyn Write, text: &str) -> ProtocolResult<()> {
        let mut out = String::with_capacity(text.len() + 4);
        if !text.is_empty() {
            for line in text.split('\n') {
                if line.starts_with('.') {
                    out.push('.');
                }
                out.push_str(line);
                out.push('\n');
            }
        }
        out.push_str(".\n");
        writer.write_all(out.as_bytes())?;
        writer.flush()?;
        Ok(())
    }

    /// Reads one framed message as raw text, without parsing it.
    pub fn read_text(&self, reader: &mut dyn BufRead) -> ProtocolResult<Option<String>> {
        let mut text = Vec::new();
        let mut line = Vec::new();
        let mut first = true;
        loop {
            line.clear();
            let budget = self.max_text_bytes.saturating_sub(text.len()) as u64 + 2;
            let read = reader.take(budget).read_until(b'\n', &mut line)?;
            if read == 0 {
                if first {
                    return Ok(None);
                }
                return Err(ProtocolError::malformed(
                    "stream ended before the '.' terminator line",
                ));
            }
            if line.last() != Some(&b'\n') {
                if read as u64 >= budget {
                    return Err(ProtocolError::LimitExceeded(format!(
                        "text message longer than {} bytes",
                        self.max_text_bytes
                    )));
                }
                return Err(ProtocolError::malformed(
                    "stream ended before the '.' terminator line",
                ));
            }
            line.pop();
            // CRLF line endings (telnet, netcat on Windows) are accepted.
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            if line == b"." {
                break;
            }
            if !first {
                text.push(b'\n');
            }
            first = false;
            let content = line.strip_prefix(b".").unwrap_or(&line);
            text.extend_from_slice(content);
        }
        String::from_utf8(text)
            .map(Some)
            .map_err(|_| ProtocolError::malformed("text message is not valid UTF-8"))
    }
}

impl LinoProtocol for TextLinoProtocol {
    fn write_document(
        &self,
        writer: &mut dyn Write,
        document: &[LiNo<String>],
    ) -> ProtocolResult<()> {
        Self::write_text(writer, &format_document(document))
    }

    fn read_document(&self, reader: &mut dyn BufRead) -> ProtocolResult<Option<LinoDocument>> {
        self.read_text(reader)?
            .map(|text| parse_document(&text).map_err(ProtocolError::from))
            .transpose()
    }
}

/// Binary links packets of [`links_notation::binary`]; every message is
/// self-delimiting, so no extra framing is needed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BinaryLinoProtocol {
    /// Optional features used when writing.
    pub options: BinaryLinoOptions,
    /// Limits applied when reading, and checked when writing.
    pub limits: DecodeLimits,
}

impl BinaryLinoProtocol {
    /// A binary protocol with every optional feature off.
    pub fn new() -> Self {
        Self::default()
    }

    /// A binary protocol with the given optional features.
    pub fn with_options(options: BinaryLinoOptions) -> Self {
        Self {
            options,
            ..Self::default()
        }
    }

    /// Encodes a document into packet bytes.
    pub fn encode(&self, document: &[LiNo<String>]) -> ProtocolResult<Vec<u8>> {
        Ok(encode_document_with_limits(document, self.options, &self.limits)?.to_bytes()?)
    }

    /// Decodes packet bytes into a document.
    pub fn decode(&self, bytes: &[u8]) -> ProtocolResult<LinoDocument> {
        let packet = LinksPacket::from_bytes(bytes, &self.limits)?;
        Ok(decode_document(&packet, &self.limits)?)
    }
}

impl LinoProtocol for BinaryLinoProtocol {
    fn write_document(
        &self,
        writer: &mut dyn Write,
        document: &[LiNo<String>],
    ) -> ProtocolResult<()> {
        writer.write_all(&self.encode(document)?)?;
        writer.flush()?;
        Ok(())
    }

    fn read_document(&self, reader: &mut dyn BufRead) -> ProtocolResult<Option<LinoDocument>> {
        LinksPacket::read_from(reader, &self.limits)?
            .map(|packet| Ok(decode_document(&packet, &self.limits)?))
            .transpose()
    }
}

/// The wire format a message arrived in, so a reply can use the same one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageFormat {
    /// [`TextLinoProtocol`].
    Text,
    /// [`BinaryLinoProtocol`] with the options read from the packet header.
    Binary(BinaryLinoOptions),
}

impl MessageFormat {
    /// A protocol that writes messages in this format.
    pub fn protocol(self, limits: ProtocolLimits) -> Box<dyn LinoProtocol> {
        match self {
            MessageFormat::Text => Box::new(TextLinoProtocol {
                max_text_bytes: limits.max_text_bytes,
            }),
            MessageFormat::Binary(options) => Box::new(BinaryLinoProtocol {
                options,
                limits: limits.binary,
            }),
        }
    }
}

/// True when `byte` starts a binary message rather than a text one.
pub fn is_binary_start(byte: u8) -> bool {
    byte & 0xF0 == BINARY_VERSION_1
}

/// Reads one message in whichever protocol the peer used.
pub fn read_any_document(
    reader: &mut dyn BufRead,
    limits: &ProtocolLimits,
) -> ProtocolResult<Option<(LinoDocument, MessageFormat)>> {
    let Some(&first) = reader.fill_buf()?.first() else {
        return Ok(None);
    };
    if !is_binary_start(first) {
        let text = TextLinoProtocol {
            max_text_bytes: limits.max_text_bytes,
        };
        return Ok(text
            .read_document(reader)?
            .map(|document| (document, MessageFormat::Text)));
    }
    LinksPacket::read_from(reader, &limits.binary)?
        .map(|packet| {
            let document = decode_document(&packet, &limits.binary)?;
            Ok((
                document,
                MessageFormat::Binary(BinaryLinoOptions::of_packet(&packet)),
            ))
        })
        .transpose()
}

/// A byte stream decorated with a [`LinoProtocol`].
#[derive(Debug)]
pub struct LinoConnection<S: Read + Write, P: LinoProtocol> {
    stream: BufReader<S>,
    protocol: P,
}

impl<S: Read + Write, P: LinoProtocol> LinoConnection<S, P> {
    /// Wraps `stream`.
    pub fn new(stream: S, protocol: P) -> Self {
        Self {
            stream: BufReader::new(stream),
            protocol,
        }
    }

    /// The protocol in use.
    pub fn protocol(&self) -> &P {
        &self.protocol
    }

    /// Sends one document.
    pub fn send(&mut self, document: &[LiNo<String>]) -> ProtocolResult<()> {
        let mut buffer = Vec::new();
        self.protocol.write_document(&mut buffer, document)?;
        let stream = self.stream.get_mut();
        stream.write_all(&buffer)?;
        stream.flush()?;
        Ok(())
    }

    /// Receives one document; `Ok(None)` when the peer closed the stream.
    pub fn receive(&mut self) -> ProtocolResult<Option<LinoDocument>> {
        self.protocol.read_document(&mut self.stream)
    }

    /// Sends `document` and waits for the reply.
    pub fn request(&mut self, document: &[LiNo<String>]) -> ProtocolResult<LinoDocument> {
        self.send(document)?;
        self.receive()?
            .ok_or_else(|| ProtocolError::malformed("connection closed before the reply"))
    }

    /// Unwraps the underlying stream.
    pub fn into_inner(self) -> S {
        self.stream.into_inner()
    }
}
