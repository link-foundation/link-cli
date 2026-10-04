//! LiNo substitution operations over TCP/IP (issue #105).
//!
//! Two interchangeable protocols carry LiNo documents between a client and a
//! server:
//!
//! - [`TextLinoProtocol`]: UTF-8 LiNo text, each message ended by a line
//!   holding only `.` (SMTP-style dot-stuffing keeps the framing unambiguous).
//! - [`BinaryLinoProtocol`]: a self-delimiting [`LinksPacket`] whose header
//!   carries the number of links and whose reference width grows with the
//!   number of links (8, 16, 32 or 64 bits). Its optional features —
//!   [external references](BinaryLinoOptions::external_references),
//!   [the sequence section](BinaryLinoOptions::sequences) and
//!   [progressive widths](BinaryLinoOptions::progressive_widths) — are all
//!   off by default and can be switched on one by one.
//!
//! Both implement [`LinoProtocol`], so a [`LinoConnection`] or a
//! [`LinksClient`] switches protocol by swapping one value. A
//! [`LinksServer`] detects the protocol of every message and answers in kind.
//!
//! A request is a substitution query, so it covers create, read, update and
//! delete; the reply lists the `(before) (after)` changes, exactly like
//! `clink --changes`. The empty request reads the whole store.
//!
//! ```
//! use link_cli::protocol::{format_document, parse_document, BinaryLinoOptions, BinaryLinoProtocol};
//!
//! let document = parse_document("() ((1 1))").unwrap();
//! let binary = BinaryLinoProtocol::with_options(
//!     BinaryLinoOptions::default().with_external_references(true),
//! );
//! let bytes = binary.encode(&document).unwrap();
//! assert_eq!(binary.decode(&bytes).unwrap(), document);
//! assert_eq!(format_document(&document), "() ((1 1))");
//! ```

mod client;
mod error;
mod format;
mod mapping;
pub mod packet;
mod protocols;
mod server;

pub use client::LinksClient;
pub use error::{ProtocolError, ProtocolResult};
pub use format::{format_document, format_link, format_reference, parse_document};
pub use mapping::{decode_document, encode_document, BinaryLinoOptions, LinoDocument};
pub use packet::{DecodeLimits, LinksPacket, Reference};
pub use protocols::{
    is_binary_start, read_any_document, BinaryLinoProtocol, LinoConnection, LinoProtocol,
    MessageFormat, TextLinoProtocol,
};
pub use server::{
    error_document, error_message, execute_request, AcceptedProtocols, LinksServer, ServerOptions,
    ShutdownHandle,
};
