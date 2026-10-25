//! Execute shared query, naming, Unicode and storage semantics in actual WASM.
#![cfg(target_arch = "wasm32")]

#[path = "../../tests/common/storage.rs"]
mod test_store;

#[path = "../../tests/common/version_metadata.rs"]
mod version_metadata;

#[path = "../../tests/query_processor_tests.rs"]
mod query_processor;

#[path = "../../tests/query_processor_change_report_tests.rs"]
mod query_processor_change_report;

#[path = "../../tests/query_processor_csharp_parity_tests.rs"]
mod query_processor_csharp_parity;

#[path = "../../tests/quoted_query_references_tests.rs"]
mod quoted_query_references;

#[path = "../../tests/unicode_sequence_converter_tests.rs"]
mod unicode_sequence_converter;

#[path = "../../tests/unicode_string_storage_tests.rs"]
mod unicode_string_storage;

#[path = "../../tests/pinned_types_decorator_tests.rs"]
mod pinned_types_decorator;

#[path = "../../tests/issue62_review_coverage_tests.rs"]
mod issue62_review_coverage;

#[path = "../../tests/lino_database_input_tests.rs"]
mod lino_database_input;

#[path = "../../tests/indexed_links_tests.rs"]
mod indexed_links;

#[path = "../../tests/storage_trait_defaults_tests.rs"]
mod storage_trait_defaults;

#[path = "../../tests/link_storage_tests.rs"]
mod link_storage;

#[path = "../../tests/named_types_decorator_tests.rs"]
mod named_types_decorator;

#[path = "../../tests/transactions_portable_clock_tests.rs"]
mod transactions_portable_clock;

#[path = "../../tests/transactions_decorator_tests.rs"]
mod transactions_decorator;

#[path = "../../tests/version_control_decorator_tests.rs"]
mod version_control_decorator;

#[path = "../../tests/transactions_commit_failure_tests.rs"]
mod transactions_commit_failure;

#[path = "../../tests/transactions_recovery_order_tests.rs"]
mod transactions_recovery_order;

#[path = "../../tests/transactions_recovery_fault_tests.rs"]
mod transactions_recovery_fault;

#[path = "../../tests/transactions_commit_decision_tests.rs"]
mod transactions_commit_decision;

#[path = "../../tests/version_initial_abort_tests.rs"]
mod version_initial_abort;

#[path = "../../tests/version_automatic_contract_tests.rs"]
mod version_automatic_contract;

#[path = "../../tests/version_publication_failure_tests.rs"]
mod version_publication_failure;

#[path = "../../tests/version_retained_history_tests.rs"]
mod version_retained_history;

#[path = "../../tests/version_movement_failure_tests.rs"]
mod version_movement_failure;

#[path = "../../tests/version_movement_recovery_tests.rs"]
mod version_movement_recovery;

#[path = "../../tests/transactions_replay_owner_tests.rs"]
mod transactions_replay_owner;
