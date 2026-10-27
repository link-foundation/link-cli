//! Addresses understood by the shared query and named-store interfaces.
use doublets::data::LinkReference;
use std::str::FromStr;

/// An ordinary link address that can also be read from a query reference.
/// The query's internal wildcard is `MAX`; a checked adapter may use a wider
/// address type to keep that marker outside its stored address range.
pub trait QueryAddress: LinkReference + FromStr {}

impl<T: LinkReference + FromStr> QueryAddress for T {}
