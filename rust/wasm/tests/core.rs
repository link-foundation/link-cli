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

#[path = "../../tests/engine_checkpoint_tests.rs"]
mod checkpoint;

#[path = "../../tests/engine_checkpoint_delta_tests.rs"]
mod checkpoint_delta;

#[path = "../../tests/engine_checkpoint_pages_tests.rs"]
mod checkpoint_pages;

#[path = "../../tests/engine_partitioned_tests.rs"]
mod partitioned;

#[path = "../../tests/engine_reclaiming_tests.rs"]
mod reclaiming;

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

#[path = "../../tests/engine_group_commit_tests.rs"]
mod group_commit;

#[path = "../../tests/engine_shared_state_tests.rs"]
mod shared_state;

#[path = "../../tests/engine_change_replay_tests.rs"]
mod change_replay;

#[path = "../../tests/engine_changes_tests.rs"]
mod changes;

#[path = "../../tests/engine_changes_fault_tests.rs"]
mod changes_fault;

#[path = "../../tests/engine_constraint_index_tests.rs"]
mod constraint_index;

#[path = "../../tests/engine_paged_tests.rs"]
mod engine_paged_tests;

#[path = "../../tests/engine_mirrored_pages_tests.rs"]
mod engine_mirrored_pages_tests;

#[path = "../../tests/engine_compaction_tests.rs"]
mod compaction;
