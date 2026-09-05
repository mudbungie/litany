//! The inbox substrate (ARCH §2.11 *Messages*).
//!
//! A **message** is content addressed to an existing agent, deposited
//! into the recipient's inbox and delivered at its next step boundary.
//! This module lands the deposit half of the channel: the executor lock
//! ([`lock`]), the create-only deposit ([`deposit`]), the
//! deposit-starts-a-driver probe and the launch it performs ([`launch`]),
//! and the `litany message` verb that orders the two ([`cli`]). This file
//! itself holds only what the inbox *is* — the directory names and the
//! agent-id arithmetic every other file reads — with each of the three
//! axes beneath it in its own file (bl-6a7c); the re-exports above keep
//! `inbox::probe_and_launch` and `inbox::cli_run` reading as they always
//! did.
//!
//! The delivery drain that moves these files into the transcript lives
//! with the executor's step loop (bl-1129,
//! [`crate::prompt::dispatch`] — a driver, not a writer). The
//! workspace-wide sweep-and-flush behind the **operator verb**
//! `litany scan` — crash-rate compensation, never wired into any driver
//! hot path (§2.11) — is [`scan`] (bl-d148, bl-5846); it, the
//! result-message return path (bl-4ce8), and the §2.11 exit protocol's
//! self-directed launch (bl-5846) ride this same substrate.
//!
//! **Writer/driver totality (§2.11).** `litany message` is a *writer*:
//! it deposits and, if it observes the recipient quiescent (the lock
//! probe succeeds), *launches* a driver and exits — launching is not
//! driving, so the probe lease is released the instant it is taken and
//! never held to step. A driver that loses the acquire exits as a clean
//! no-op. Because no verb combines the two arms, the losing path is the
//! same code as the uncontended one.

pub mod baton;
pub mod cli;
pub mod deposit;
pub mod launch;
pub mod lock;
pub mod scan;

#[cfg(test)]
mod tests;

pub use cli::cli_run;
pub use deposit::{DepositError, Epitaph, deposit, deposit_result};
pub use launch::{AdvanceLauncher, Launcher, ProbeOutcome, probe_and_launch};
pub use lock::{ExecutorLock, try_acquire};
pub use scan::scan;

use std::path::{Path, PathBuf};

/// Workspace-root directory holding every agent's inbox, namespaced by
/// agent id exactly like `steps/` (§2.2, §2.11). Outside every worktree.
pub const INBOX_DIR: &str = "inbox";

/// The reserved sender token for a deposit made by the user rather than
/// by an agent (§2.11 — `<sender>` is an agent id or `user`).
pub const USER_SENDER: &str = "user";

/// The per-agent inbox directory `<workspace>/inbox/<agent-id>/` — the
/// deposit target and the executor lock's home (§2.11).
pub fn inbox_dir(workspace: &Path, agent_id: &str) -> PathBuf {
    workspace.join(INBOX_DIR).join(agent_id)
}

/// The parent agent's id — `agent_id` minus its last descent segment
/// (§2.11 "the parent's address is the agent's own id minus its last
/// descent segment") — or `None` when `agent_id` is a root (it has no
/// parent). An agent id is a hyphenated descent of `<ts>-<short>`
/// segments (§2.3), and both the compact timestamp and the short id are
/// hyphen-free (`clock.rs`), so each segment is exactly two
/// hyphen-delimited tokens: a root is two tokens, and stripping the last
/// segment removes the trailing two. This is the same token arithmetic
/// [`crate::prompt::budget::derive::depth`] already relies on.
pub fn parent_of(agent_id: &str) -> Option<String> {
    let tokens: Vec<&str> = agent_id.split('-').collect();
    if tokens.len() <= 2 {
        return None;
    }
    Some(tokens[..tokens.len() - 2].join("-"))
}
