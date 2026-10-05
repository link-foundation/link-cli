//! LiNo substitution operations over TCP/IP (issue #105).
//!
//! Two interchangeable protocols carry LiNo documents between a client and a
//! server:
//!
//! - [`TextLinoProtocol`]: UTF-8 LiNo text, each message ended by a line
//!   holding only `.` (SMTP-style dot-stuffing keeps the framing unambiguous).
//! - [`BinaryLinoProtocol`]: a self-delimiting [`LinksPacket`] made of
//!   sections of links that share one reference width (1, 2, 4 or 8 bytes)
//!   and one [arity range](ArityRange). By default every link is a doublet
//!   and the whole packet uses the narrowest width that fits; the options
//!   [external references](BinaryLinoOptions::external_references),
//!   [arity](BinaryLinoOptions::arity) (for example `2..3` or `1..`) and
//!   [packed widths](BinaryLinoOptions::packed_widths) can be switched on
//!   one by one.
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
pub mod links_operations;
mod mapping;
pub mod packet;
mod protocols;
mod remote_links;
mod server;

pub use client::LinksClient;
pub use error::{ProtocolError, ProtocolResult};
pub use format::{format_document, format_link, format_reference, parse_document};
pub use links_operations::LinksOperation;
pub use mapping::{decode_document, encode_document, BinaryLinoOptions, LinoDocument};
pub use packet::{ArityRange, DecodeLimits, LinksPacket, Reference, Section};
pub use protocols::{
    is_binary_start, read_any_document, BinaryLinoProtocol, LinoConnection, LinoProtocol,
    MessageFormat, TextLinoProtocol,
};
pub use remote_links::RemoteLinks;
pub use server::{
    error_document, error_message, execute_request, AcceptedProtocols, LinksServer, ServerOptions,
    ShutdownHandle,
};
