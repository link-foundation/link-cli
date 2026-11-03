//! A TCP client for [`LinksServer`](super::LinksServer).

use super::error::{ProtocolError, ProtocolResult};
use super::protocols::{LinoConnection, LinoProtocol};
use super::server::error_message;
use links_notation::binary::{format_document, parse_document, LinoDocument};
use links_notation::LiNo;
use std::net::{TcpStream, ToSocketAddrs};

/// Sends substitution queries to a LiNo server over any [`LinoProtocol`].
#[derive(Debug)]
pub struct LinksClient {
    connection: LinoConnection<TcpStream, Box<dyn LinoProtocol>>,
}

impl LinksClient {
    /// Connects to `address` using `protocol` for every message.
    pub fn connect(
        address: impl ToSocketAddrs,
        protocol: impl LinoProtocol + 'static,
    ) -> ProtocolResult<Self> {
        let stream = TcpStream::connect(address)?;
        let _ = stream.set_nodelay(true);
        Ok(Self {
            connection: LinoConnection::new(stream, Box::new(protocol)),
        })
    }

    /// Sends a parsed document and returns the reply document.
    ///
    /// An `(error: 'message')` reply becomes [`ProtocolError::Remote`].
    pub fn request(&mut self, document: &LinoDocument) -> ProtocolResult<LinoDocument> {
        let reply = self.connection.request(document)?;
        if let Some(message) = error_message(&reply) {
            return Err(ProtocolError::Remote(message.to_string()));
        }
        Ok(reply)
    }

    /// Runs a LiNo substitution query; the empty query reads every link.
    pub fn query(&mut self, query: &str) -> ProtocolResult<LinoDocument> {
        self.request(&parse_document(query)?)
    }

    /// Like [`query`](Self::query), returning the reply as canonical text.
    pub fn query_text(&mut self, query: &str) -> ProtocolResult<String> {
        Ok(format_document(&self.query(query)?))
    }

    /// Sends `(auth: token)`: every later request of the connection runs as
    /// the subject the server gives the token, which this returns (issue
    /// #149). A token the server does not know is a [`ProtocolError::Remote`].
    pub fn authenticate(&mut self, token: &str) -> ProtocolResult<u32> {
        let reply = self.request(&vec![LiNo::Link {
            id: Some("auth".to_string()),
            values: vec![LiNo::Ref(token.to_string())],
        }])?;
        if let [LiNo::Link {
            id: Some(id),
            values,
        }] = reply.as_slice()
        {
            if let [LiNo::Ref(subject)] = values.as_slice() {
                if id == "subject" && subject.bytes().all(|byte| byte.is_ascii_digit()) {
                    if let Ok(subject) = subject.parse() {
                        return Ok(subject);
                    }
                }
            }
        }
        Err(ProtocolError::Malformed(format!(
            "expected a subject reply, found {}",
            format_document(&reply)
        )))
    }
}
