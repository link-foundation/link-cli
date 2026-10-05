//! A TCP client for [`LinksServer`](super::LinksServer).

use super::error::{ProtocolError, ProtocolResult};
use super::format::{format_document, parse_document};
use super::mapping::LinoDocument;
use super::protocols::{LinoConnection, LinoProtocol};
use super::server::error_message;
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
}
