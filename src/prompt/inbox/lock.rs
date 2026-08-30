//! The executor lock (ARCH §2.11 *The executor lock*).
//!
//! `flock(2)` on the agent's **inbox directory** fd, acquired
//! non-blocking when an executor starts and held for the whole step
//! loop. The lock is kernel state bound to process lifetime: released by
//! the kernel on any death, observable but never written — there is no
//! stale-lock cleanup because there is nothing on disk to go stale
//! (PRINCIPLES "Single source of truth"). The inbox directory lives at
//! the workspace root and persists across worktree teardown (§2.3
//! step 6), so the lock's home outlives the substrate's materialization.
//!
//! Two open file descriptions on the same directory contend even inside
//! one process (`flock(2)`: descriptors from separate `open` calls are
//! treated independently), so `try_acquire` is a true mutual-exclusion
//! probe: the caller who wins holds the lease, everyone else observes
//! `None` and steps aside (Writer/driver totality, §2.11).
//!
//! **Release is explicit `LOCK_UN`, not a bare close.** The lock rides
//! the *open file description*, and closing one fd for it releases the
//! lease only once **every** fd naming that description is gone. Spawning
//! a subprocess transiently makes more of them: `fork`/`clone` copies the
//! whole fd table, and close-on-exec fires at `execve`, not at the fork —
//! so between a spawn and its exec, a child that has nothing to do with
//! this branch holds the lease too. Any spawn anywhere in the process
//! (git, the provider adapter, a tool, a detached launch) opens that
//! window, and a lease released by close inside it stays kernel-held
//! until the unrelated child execs. A subsequent probe then reads
//! `EWOULDBLOCK` and the caller concludes *another executor drives this
//! branch* — a lie that turns a driver into a silent no-op (§2.11
//! Writer/driver totality) and a sweep candidate into a live agent (§8).
//! `flock(fd, LOCK_UN)` clears the lock from the description itself, so
//! every copy of it loses the lease at once; [`ExecutorLock`]'s `Drop`
//! makes that the only way a lease is ever given up. Nothing is written
//! either way — the release is still pure kernel state, and process death
//! still releases (the kernel drops the description with its last fd).

use std::fs::File;
use std::io;
use std::os::fd::AsRawFd;
use std::path::Path;

/// A held executor lease. Dropping it releases the `flock` explicitly
/// (`LOCK_UN`) and then closes the fd; nothing is written on release. The
/// `File` is the whole state — the guard exists only to tie the kernel
/// lease to a Rust lifetime.
#[derive(Debug)]
pub struct ExecutorLock {
    // Held solely to keep the fd (and thus the lease) alive; read only
    // by the §6 exec baton, which publishes the number as LITANY_LOCK_FD.
    fd: File,
}

impl Drop for ExecutorLock {
    /// Release the lease on the open file description, not merely on this
    /// fd (module docs): a concurrent spawn's pre-`exec` child shares the
    /// description, and a close-only release would leave the lease held
    /// until that unrelated child execs.
    fn drop(&mut self) {
        // SAFETY: `flock` takes a valid fd (owned by `self.fd`, alive
        // until this guard's fields drop) and a flag constant; it has no
        // memory effects. `LOCK_UN` on an fd whose lock is already gone
        // is a no-op, so there is no failure to surface.
        unsafe { libc::flock(self.fd.as_raw_fd(), libc::LOCK_UN) };
    }
}

impl ExecutorLock {
    /// The lease fd's raw number — what the §6 exec baton publishes as
    /// `LITANY_LOCK_FD` for the successor hop to adopt. The fd stays
    /// owned by the guard; the baton leaks the guard just before exec so
    /// the open file description (and the flock riding it) survives it.
    pub fn as_raw_fd(&self) -> std::os::fd::RawFd {
        self.fd.as_raw_fd()
    }
}

/// Try to acquire the executor lock for the agent whose inbox is
/// `inbox_dir`. Non-blocking: `Ok(Some(_))` means the lease is now held
/// by the returned guard; `Ok(None)` means another executor holds it
/// (the branch is being driven); `Err` is an I/O failure opening the
/// inbox fd. The inbox directory is created on demand — a fresh agent
/// with no deposited messages still has a lock home (§2.3 step 6).
pub fn try_acquire(inbox_dir: &Path) -> io::Result<Option<ExecutorLock>> {
    let fd = open_inbox_fd(inbox_dir)?;
    lock_or_none(fd)
}

/// Open (creating if needed) the inbox directory and return an fd on it.
/// The only branch here is `create_dir_all`'s; `File::open` on a
/// just-ensured directory returns its `Result` straight through.
fn open_inbox_fd(inbox_dir: &Path) -> io::Result<File> {
    std::fs::create_dir_all(inbox_dir)?;
    File::open(inbox_dir)
}

/// `flock(LOCK_EX | LOCK_NB)` the fd, mapping the outcome to a guard.
/// The error interpretation is factored into [`interpret_lock`] so all
/// three arms are unit-testable without provoking a real syscall
/// failure.
pub(super) fn lock_or_none(fd: File) -> io::Result<Option<ExecutorLock>> {
    // SAFETY: `flock` takes a valid fd (owned by `fd`, alive for the
    // call) and a flag constant; it has no memory effects.
    let ret = unsafe { libc::flock(fd.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    interpret_lock(fd, ret, io::Error::last_os_error())
}

/// Classify a `flock` return: `0` → lease held; `EWOULDBLOCK` → someone
/// else drives; any other errno → propagate. Kept pure (takes the fd,
/// the raw return, and the captured errno) so the Err arm is reachable
/// in a test without a genuine syscall failure.
pub(super) fn interpret_lock(
    fd: File,
    ret: i32,
    err: io::Error,
) -> io::Result<Option<ExecutorLock>> {
    if ret == 0 {
        Ok(Some(ExecutorLock { fd }))
    } else if err.raw_os_error() == Some(libc::EWOULDBLOCK) {
        Ok(None)
    } else {
        Err(err)
    }
}
