//! Unit tests for [`super`]: the process-wide SIGTERM handler and the
//! pre-exec hook, each called in-process so its lines land in the
//! coverage numerator. The cascade itself is driven end-to-end through
//! the two built-ins that spawn children ([`super::super::bash`]'s
//! `run_with` tests, which pass a caller-owned flag and a sub-second
//! deadline, and the `python` interpreter's).

use super::*;

#[test]
fn install_sigterm_handler_is_idempotent_and_flag_accessible() {
    install_sigterm_handler();
    install_sigterm_handler();
    let flag = sigterm_flag();
    // Default state — we have not raise()d anything, so the flag is
    // false at this point.
    assert!(!flag.load(Ordering::SeqCst));
}

#[test]
fn signal_handler_sets_flag() {
    SIGTERM_FLAG.store(false, Ordering::SeqCst);
    on_sigterm(libc::SIGTERM);
    assert!(SIGTERM_FLAG.load(Ordering::SeqCst));
    SIGTERM_FLAG.store(false, Ordering::SeqCst);
}

#[test]
fn enter_own_process_group_is_an_idempotent_success() {
    // The pre-exec hook, called in-process: `setpgid(0, 0)` makes the
    // caller its own group leader, and repeating it on a leader is a
    // no-op success. The cascade tests prove the group semantics
    // end-to-end; this call is what puts the hook's own lines in the
    // coverage numerator (counters incremented in the forked child are
    // lost at exec).
    enter_own_process_group().unwrap();
    enter_own_process_group().unwrap();
}
