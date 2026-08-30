//! The exec baton's lease handoff (ARCH §6 "The exec baton carries the
//! lease", §2.11 executor lock).
//!
//! The §2.11 flock lease lives on an open file description, which
//! survives `execve` — but the harness opens fds close-on-exec by
//! default, so inheritance across the advance→advance exec is deliberate
//! work: the predecessor hop clears `FD_CLOEXEC` on the lock fd
//! immediately before exec ([`successor_command`]) and publishes its
//! number as [`LOCK_FD_ENV`]; the successor **adopts** the fd instead of
//! reacquiring ([`take_lease`] → [`adopt`]) — validating it by fstat
//! against the inbox directory, re-asserting the flock (idempotent on
//! the same open file description), and restoring `FD_CLOEXEC` so the
//! lease never leaks into the tool and adapter subprocesses the hop
//! spawns. A bad fd is **declined loudly**, never papered over with a
//! fresh acquire: a mismatched fd means a defective launcher, and a
//! silent reacquire would mask it (PRINCIPLES "Decline illegal
//! operations").

use super::lock::{self, ExecutorLock};
use std::ffi::OsStr;
use std::fs::File;
use std::io;
use std::os::fd::{FromRawFd, IntoRawFd, RawFd};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The env var a hop publishes the lock fd number under, read only by
/// the exec'd successor (§6). Every *other* spawn scrubs it: a launched
/// driver acquires, only an exec'd successor adopts.
pub const LOCK_FD_ENV: &str = "LITANY_LOCK_FD";

/// Every way adopting a predecessor's lease fd can fail (§6). All are
/// declined loudly by the caller — never resolved by reacquiring.
#[derive(Debug, thiserror::Error)]
pub enum AdoptError {
    #[error("{LOCK_FD_ENV}={0:?} is not a valid fd number")]
    Parse(String),
    #[error("fstat adopted fd {fd}: {source}")]
    Fstat {
        fd: RawFd,
        #[source]
        source: io::Error,
    },
    #[error("stat inbox {inbox}: {source}")]
    InboxStat {
        inbox: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("adopted fd {fd} is not the inbox directory {inbox} (device/inode mismatch)")]
    Mismatch { fd: RawFd, inbox: PathBuf },
    #[error("restore close-on-exec on adopted fd {fd}: {source}")]
    Cloexec {
        fd: RawFd,
        #[source]
        source: io::Error,
    },
    #[error("re-assert flock on adopted fd {fd}: {source}")]
    Flock {
        fd: RawFd,
        #[source]
        source: io::Error,
    },
}

/// Every way the take-the-lease phase (§6 hop step 1) can fail.
#[derive(Debug, thiserror::Error)]
pub enum LeaseError {
    #[error(transparent)]
    Adopt(#[from] AdoptError),
    #[error("acquire executor lock: {0}")]
    Acquire(#[source] io::Error),
}

/// Take the lease for one hop (§6): adopt the fd a predecessor published
/// under [`LOCK_FD_ENV`], else try-acquire the §2.11 executor lock.
/// `Ok(None)` means another executor drives the branch — the clean no-op
/// of Writer/driver totality (§2.11).
pub fn take_lease(
    lease_env: Option<&OsStr>,
    inbox_dir: &Path,
) -> Result<Option<ExecutorLock>, LeaseError> {
    match lease_env {
        Some(v) => Ok(adopt(v, inbox_dir)?),
        None => lock::try_acquire(inbox_dir).map_err(LeaseError::Acquire),
    }
}

/// Adopt the lease fd named by `env_val` (§6): parse, fstat-validate
/// against the inbox directory, restore `FD_CLOEXEC`, re-assert the
/// flock. `Ok(None)` — the re-assert observing another holder — resolves
/// to the ordinary already-driven no-op.
pub fn adopt(env_val: &OsStr, inbox_dir: &Path) -> Result<Option<ExecutorLock>, AdoptError> {
    let fd: RawFd = env_val
        .to_str()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| AdoptError::Parse(env_val.to_string_lossy().into_owned()))?;
    // SAFETY: the exec'd image's fd table is exactly what the
    // predecessor left; ownership of the published fd is the handoff.
    let file = unsafe { File::from_raw_fd(fd) };
    if let Err(e) = validate(&file, fd, inbox_dir) {
        // Never close an fd we never proved ours: a lying env var could
        // name an unrelated open fd, and dropping would steal it.
        let _ = file.into_raw_fd();
        return Err(e);
    }
    // Validated ours: restore close-on-exec so the lease never rides
    // into tool/adapter subprocesses (§6); the next hop's baton clears
    // it again deliberately.
    interpret_cloexec(fd, set_fd_flags(fd, libc::FD_CLOEXEC))?;
    // Re-assert the flock: idempotent on the same open file description;
    // a foreign-but-right-inode fd resolves to acquire-or-already-driven
    // instead of proceeding unlocked.
    interpret_flock(fd, lock::lock_or_none(file))
}

/// Map a close-on-exec restore outcome to its adopt arm. The failure is
/// unreachable on a validated fd (fstat already proved it live), so the
/// mapper is pure and directly tested — the `lock::interpret_lock`
/// pattern.
fn interpret_cloexec(fd: RawFd, r: io::Result<()>) -> Result<(), AdoptError> {
    match r {
        Ok(()) => Ok(()),
        Err(source) => Err(AdoptError::Cloexec { fd, source }),
    }
}

/// Map a flock re-assert outcome to its adopt arm (same pattern as
/// [`interpret_cloexec`]).
fn interpret_flock(
    fd: RawFd,
    r: io::Result<Option<ExecutorLock>>,
) -> Result<Option<ExecutorLock>, AdoptError> {
    match r {
        Ok(lock) => Ok(lock),
        Err(source) => Err(AdoptError::Flock { fd, source }),
    }
}

/// fstat the adopted fd and require the same device and inode as the
/// agent's inbox directory (§6) — the validation that makes a defective
/// launcher loud instead of latent.
fn validate(file: &File, fd: RawFd, inbox_dir: &Path) -> Result<(), AdoptError> {
    let fd_meta = file
        .metadata()
        .map_err(|source| AdoptError::Fstat { fd, source })?;
    let dir_meta = std::fs::metadata(inbox_dir).map_err(|source| AdoptError::InboxStat {
        inbox: inbox_dir.to_path_buf(),
        source,
    })?;
    if fd_meta.dev() != dir_meta.dev() || fd_meta.ino() != dir_meta.ino() {
        return Err(AdoptError::Mismatch {
            fd,
            inbox: inbox_dir.to_path_buf(),
        });
    }
    Ok(())
}

/// Prepare the successor hop's `Command` (§6): clear close-on-exec on
/// the lock fd — the one deliberate inheritance in the system — publish
/// its number as [`LOCK_FD_ENV`], and hand back the command for the
/// caller to `exec`. The lease guard is deliberately leaked: this
/// process either becomes the successor (exec) or exits on the exec
/// failing, and the kernel releases the flock at either boundary.
pub fn successor_command(
    exe: &Path,
    workspace: &Path,
    agent_id: &str,
    lease: ExecutorLock,
) -> io::Result<Command> {
    let fd = lease.as_raw_fd();
    set_fd_flags(fd, 0)?;
    std::mem::forget(lease);
    let mut cmd = Command::new(exe);
    cmd.arg("advance")
        .arg(workspace)
        .arg(agent_id)
        .env(LOCK_FD_ENV, fd.to_string());
    Ok(cmd)
}

/// `fcntl(fd, F_SETFD, flags)` with errno surfaced. `flags` is `0`
/// (inheritable, pre-exec only) or `FD_CLOEXEC` (the default posture,
/// restored on adoption).
fn set_fd_flags(fd: RawFd, flags: libc::c_int) -> io::Result<()> {
    // SAFETY: fcntl F_SETFD takes a raw fd and an int flag word; it has
    // no memory effects. A closed/invalid fd yields -1/EBADF, surfaced.
    let ret = unsafe { libc::fcntl(fd, libc::F_SETFD, flags) };
    if ret == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
