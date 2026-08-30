//! Tests for the tool executor (ARCH §3.3). Split by axis so each
//! file stays under the 300-line cap.
//!
//! - [`fixtures`]: shared clock + test-side helpers for laying down
//!   fixture tool scripts in a tempdir-rooted harness root.
//! - [`types`]: round-trips and small invariants for the on-disk
//!   record types and helpers in `super::super`.
//! - [`resolve`]: §3.3 resolution order — harness-root, PATH, and the
//!   injected-driver-target third hop. There is no not-found case: the
//!   third hop always resolves (§2.11 injected target).
//! - [`happy`]: end-to-end stdio contract — exit 0, exit non-zero,
//!   stderr concat-on-error, on-disk record shape.
//! - [`bounded`]: the §3.3 bounded transcript projection — streams
//!   bounded independently inside the envelope, full record intact.
//! - [`cascade`]: SIGTERM-then-SIGKILL semantics, and the "tool died
//!   from a signal not under harness control" §2.10 fault.
//! - [`errors`]: failure modes of resolution, spawn, and disk-record
//!   I/O.
//! - [`moved_cwd`]: the working-directory mark at the spawn boundary —
//!   the worktree as default cwd, a `cd` that moved it, and a mark whose
//!   directory has since gone.
//! - [`etxtbsy`]: the "text file busy" retry envelope around a spawn,
//!   both arms, on waits that cannot be closed by machine load.
//! - [`injection`]: the host injection seam (ARCH §3.3 *Host-injected
//!   tools*) — a test embedder that declares a tool and routes it,
//!   asserted to be indistinguishable downstream from a spawned one.
//! - [`injection_scope`]: that seam's scope after bl-a00a — an installed
//!   host answers every name, an installed binary's included, and a fan
//!   with it.
//! - [`bash_tool`], [`read_file_tool`]: end-to-end through the
//!   cargo-built `litany` binary (the §3.3 third hop), injected as the
//!   driver target via [`crate::test_support::litany_binary`].

mod bash_tool;
mod batch;
mod bounded;
mod cascade;
mod errors;
mod etxtbsy;
mod fixtures;
mod happy;
mod injection;
mod injection_scope;
mod moved_cwd;
mod read_file_tool;
mod resolve;
mod types;
