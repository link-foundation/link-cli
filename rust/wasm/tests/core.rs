//! Run the portable native engine contract tests in an actual WASM runtime.
//! Native filesystem, threads and panic-unwind probes retain their native tests.
#![cfg(target_arch = "wasm32")]

#[path = "../../tests/engine_strategies_tests.rs"]
mod strategies;

#[path = "../../tests/engine_session_tests.rs"]
mod session;

#[path = "../../tests/engine_branch_tests.rs"]
mod branch;

#[path = "../../tests/engine_branch_fault_tests.rs"]
mod branch_fault;

#[path = "../../tests/engine_boundary_tests.rs"]
mod boundary;

#[path = "../../tests/engine_cluster_tests.rs"]
mod cluster;

#[path = "../../tests/engine_partitioned_tests.rs"]
mod partitioned;

#[path = "../../tests/engine_readset_tests.rs"]
mod readset;

#[path = "../../tests/engine_delta_tests.rs"]
mod delta;

#[path = "../../tests/engine_journal_tests.rs"]
mod journal;

#[path = "../../tests/engine_portable_recovery_tests.rs"]
mod portable_recovery;

#[path = "../../tests/engine_pessimistic_tests.rs"]
mod pessimistic;
