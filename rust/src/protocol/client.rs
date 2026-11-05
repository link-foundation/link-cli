//! A TCP client for [`LinksServer`](super::LinksServer).

use super::error::{ProtocolError, ProtocolResult};
use super::links_operations::is_notification;
use super::protocols::{LinoConnection, LinoProtocol};
use super::server::error_message;
use links_notation::binary::{format_document, parse_document, LinoDocument};
use links_notation::LiNo;
use std::collections::VecDeque;
use std::io;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

/// Sends substitution queries to a LiNo server over any [`LinoProtocol`].
///
/// A server pushes the changes a connection subscribed to between its
/// replies; a pushed document starts with `(notify: N)`.
/// [`request`](Self::request) sets the pushes it meets aside for
/// [`receive_notification`](Self::receive_notification), so replies and
/// pushes never get mixed up.
#[derive(Debug)]
pub struct LinksClient {
    connection: LinoConnection<TcpStream, Box<dyn LinoProtocol>>,
    notifications: VecDeque<LinoDocument>,
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
            notifications: VecDeque::new(),
        })
    }

    /// Sends a parsed document and returns the reply document.
    ///
    /// An `(error: 'message')` reply becomes [`ProtocolError::Remote`].
    pub fn request(&mut self, document: &LinoDocument) -> ProtocolResult<LinoDocument> {
        self.connection.send(document)?;
        loop {
            let reply = self
                .connection
                .receive()?
                .ok_or_else(|| ProtocolError::malformed("connection closed before the reply"))?;
            if is_notification(&reply) {
                self.notifications.push_back(reply);
                continue;
            }
            if let Some(message) = error_message(&reply) {
                return Err(ProtocolError::Remote(message.to_string()));
            }
            return Ok(reply);
        }
    }

    /// The number of pushes received and not yet taken by
    /// [`receive_notification`](Self::receive_notification).
    pub fn pending_notifications(&self) -> usize {
        self.notifications.len()
    }

    /// The next document the server pushed, `(notify: N)` and its changes,
    /// waiting up to `timeout` for one; `Ok(None)` when none arrived in time.
    ///
    /// Fails when the connection closed, or when the server sent something
    /// other than a push while no request was waiting.
    pub fn receive_notification(
        &mut self,
        timeout: Duration,
    ) -> ProtocolResult<Option<LinoDocument>> {
        if let Some(queued) = self.notifications.pop_front() {
            return Ok(Some(queued));
        }
        if !self.connection.wait_for_message(timeout)? {
            return Ok(None);
        }
        let document = self.connection.receive()?.ok_or_else(|| {
            ProtocolError::Io(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "connection closed",
            ))
        })?;
        if is_notification(&document) {
            Ok(Some(document))
        } else {
            Err(ProtocolError::malformed(format!(
                "the server sent a reply nobody asked for: {}",
                format_document(&document)
            )))
        }
    }

    /// Runs a LiNo substitution query; the empty query reads every link.
    pub fn query(&mut self, query: &str) -> ProtocolResult<LinoDocument> {
        self.request(&parse_document(query)?)
    }

    /// Sends original query text without losing quoted-reference kinds.
    /// The peer must support [`super::query_document`]; legacy query behavior
    /// remains available through [`Self::query`]. No mutation is retried using
    /// a different wire representation after an error or unknown reply.
    pub fn query_lossless(&mut self, query: &str) -> ProtocolResult<LinoDocument> {
        self.request(&super::server::query_document(query))
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
