//! In-crate end-to-end tests that spawn the cargo-built `litany` binary
//! *and* reach into private machinery to build fixtures (authoring a
//! config commit, driving `RealGit`, bundling/replaying an archive,
//! holding the executor lock). Migrated in from `tests/` when the library
//! surface was narrowed to [`crate::cmd`] (§3.4): once those helpers went
//! private, an external integration test could no longer name them, so
//! these live in-crate as `#[cfg(test)]` modules. The binary they spawn is
//! resolved via [`crate::test_support::litany_binary`].
//!
//! Tests that only spawn the binary (no private fixture) stay in `tests/`.

mod advance_cli;
mod bundle_replay_cli;
mod compaction_wire;
mod delete_cli;
mod invoke_cli;
mod message_cli;
mod poll;
mod prompt_adapter_failure;
mod prompt_end_to_end;
mod prompt_fork_point;
mod prompt_retry;
mod python_cli;
mod replay_drive;
mod scan_cli;
mod stop_children;
mod stop_cli;
mod stop_common;
mod stop_idempotence;
