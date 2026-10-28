//! Run the portable model and protocol contracts inside an actual WASM runtime.
#![cfg(target_arch = "wasm32")]

#[path = "../../tests/cli_argument_contract_tests.rs"]
mod cli_arguments;

#[path = "../../tests/changes_simplifier_tests.rs"]
mod changes_simplifier;

#[path = "../../tests/link_tests.rs"]
mod link;

#[path = "../../tests/lino_link_tests.rs"]
mod lino_link;

#[path = "../../tests/parser_tests.rs"]
mod parser;

#[path = "../../tests/protocol_transport_tests.rs"]
mod protocol_transport;

#[path = "../../tests/protocol_packet_tests.rs"]
mod protocol_packet;
