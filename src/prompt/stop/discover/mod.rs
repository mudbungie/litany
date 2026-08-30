//! Pid discovery for `litany stop` (Linux `/proc` scan).
//!
//! Resolves "the harness driving `<agent>`" by the **executor lock**
//! (ARCH §2.11): it scans `/proc/<pid>/fd/*` symlinks for the absolute
//! path of the agent's inbox directory — the `flock` home the executor
//! opens at step-loop start and holds for the whole loop. That fd is the
//! *is-anyone-driving* signal by construction: held across tool
//! execution, inbox drains, and retry backoff alike (§2.11), so
//! discovery lands whenever an executor is alive, not only while a model
//! call is in flight. No access-mode filter — the lock fd is opened
//! read-only (`File::open`, `inbox/lock.rs`), so a "writer" test would
//! reject the very fd we are looking for; the inbox directory is
//! namespaced per agent (§2.11), so any process holding it open is that
//! agent's executor.
//!
//! The trait is `&dyn`-shaped so tests pass a stub and production pays
//! the directory scan only when actually called. See [`super::tests`]
//! for fixture usage.
//!
//! **A discovered pgid is only trusted once it equals the holder's own
//! pid** (§2.9 "Discovery trusts a pgid only from a group leader"). Every
//! driver makes itself a process-group leader at startup — `setpgid(0, 0)`
//! for `litany prompt`, `setsid` for a detached `litany advance` — so a
//! settled executor's pgid *is* its pid. Before that lands, `/proc/<pid>/stat`
//! still reports the group the executor **inherited from whoever spawned
//! it**: the operator's shell in production, the coverage runner under
//! `make check`. Signalling that reading kills the wrong tree, so a
//! non-leader reading is read as *not yet*, retried a bounded number of
//! times, and then refused rather than signalled.
//!
//! Linux only — `/proc` is not portable to Darwin or Windows. ARCH
//! §2.9 calls Linux out as the verified platform; portability deltas
//! are a v0.6+ concern.

use std::io;
use std::path::Path;
use std::time::Duration;

/// How many times a discovered holder's `/proc/<pid>/stat` is re-read
/// after a non-leader reading, waiting for its `setpgid`/`setsid` to
/// land, before the reading is refused. A **count**, not a wall-clock
/// deadline: the race this rides out only happens under load, and a
/// deadline measured under load reports the load rather than the race
/// (bl-1c2e, bl-7a3f).
const LEADER_RETRIES: u32 = 50;

/// Backoff between those re-reads. Sized for a fork/exec transition,
/// not for a scheduling stall — the attempt count is the budget.
const LEADER_BACKOFF: Duration = Duration::from_millis(10);

/// "Find the pgid of the process holding `inbox_dir`'s fd open" — the
/// executor lock (§2.11). `None` means no holder found: the executor has
/// already exited or has not yet acquired the lock. The pgid (== leader
/// pid for a setpgid'd harness, ARCH §2.9 cascade) is what
/// [`super::cascade`] signals; an `Err` means a holder was found whose
/// pgid never became its own pid, which is refused rather than signalled.
pub trait PgidFinder {
    fn find_holder_pgid(&self, inbox_dir: &Path) -> io::Result<Option<i32>>;
}

/// Production [`PgidFinder`] backed by `/proc`.
#[derive(Debug, Clone)]
pub struct ProcFsFinder {
    proc_root: std::path::PathBuf,
    leader_retries: u32,
    leader_backoff: Duration,
}

impl Default for ProcFsFinder {
    fn default() -> Self {
        Self {
            proc_root: std::path::PathBuf::from("/proc"),
            leader_retries: LEADER_RETRIES,
            leader_backoff: LEADER_BACKOFF,
        }
    }
}

impl ProcFsFinder {
    /// Override the procfs root — tests point at a fixture tree.
    #[cfg(test)] // test-only builder
    pub fn with_root(proc_root: std::path::PathBuf) -> Self {
        Self {
            proc_root,
            ..Self::default()
        }
    }

    /// Override the leader-invariant retry budget — tests that assert
    /// the refusal want the retries exhausted without the production
    /// wait, and tests that assert the retry want a budget no plausible
    /// scheduling stall can outlast (bl-7a3f).
    #[cfg(test)] // test-only builder
    pub fn with_leader_retry(self, retries: u32, backoff: Duration) -> Self {
        Self {
            leader_retries: retries,
            leader_backoff: backoff,
            ..self
        }
    }

    /// The pgid of `pid`, accepted only once it equals `pid` itself.
    ///
    /// A process-group leader's pgid is its own pid, and every executor
    /// makes itself one at startup (§2.9). Any other reading names a
    /// group the executor merely inherited — its spawner's — and
    /// `kill(-pgid, ...)` against it would fell the spawner's whole
    /// tree. Re-read a bounded number of times (the invariant is
    /// normally true on the first read; the retry covers the window
    /// between fork and the child's own `setpgid`), then refuse.
    fn leader_pgid(&self, pid: i32) -> io::Result<i32> {
        let mut pgid = read_pgid(&self.proc_root, pid)?;
        let mut retries = self.leader_retries;
        while pgid != pid && retries > 0 {
            std::thread::sleep(self.leader_backoff);
            pgid = read_pgid(&self.proc_root, pid)?;
            retries -= 1;
        }
        if pgid == pid {
            return Ok(pgid);
        }
        Err(io::Error::other(format!(
            "pid {pid} holds the agent's inbox lock but reports process group \
             {pgid} instead of its own pid: it is not a group leader, so that \
             group is one litany stop does not own (the executor's \
             setpgid/setsid has not landed, or failed — ARCH §2.9). Refusing to \
             signal it; re-run `litany stop` once the executor has settled."
        )))
    }
}

impl PgidFinder for ProcFsFinder {
    fn find_holder_pgid(&self, inbox_dir: &Path) -> io::Result<Option<i32>> {
        // Canonicalize so the symlink-target compare is exact —
        // `/proc/<pid>/fd/<n>` resolves to a fully-resolved path.
        let target = match std::fs::canonicalize(inbox_dir) {
            Ok(p) => p,
            // The inbox dir may not exist yet (a fresh agent whose
            // executor has not opened it). Treat as no holder.
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        // `filter_map(Result::ok)` drops entries that error mid-walk
        // (racing with kernel pid teardown).
        for entry in std::fs::read_dir(&self.proc_root)?.filter_map(Result::ok) {
            let Some(pid) = parse_pid_dir_name(&entry.file_name()) else {
                continue;
            };
            if pid_holds(&entry.path(), &target) {
                return self.leader_pgid(pid).map(Some);
            }
        }
        Ok(None)
    }
}

fn parse_pid_dir_name(name: &std::ffi::OsStr) -> Option<i32> {
    name.to_str().and_then(|s| s.parse::<i32>().ok())
}

/// Does any fd under `<proc_pid>/fd/` resolve to `target`? No access-
/// mode filter: the executor lock fd is opened read-only, so matching a
/// held directory fd — not a *writable* one — is the whole test.
fn pid_holds(proc_pid: &Path, target: &Path) -> bool {
    let fd_dir = proc_pid.join("fd");
    let entries = match std::fs::read_dir(&fd_dir) {
        Ok(e) => e,
        // Most pids will refuse the read (different uid, kernel
        // thread, raced teardown). The fd-scan is opportunistic —
        // pids we can't introspect simply don't match.
        Err(_) => return false,
    };
    // As above: entries that error mid-walk (raced teardown) are
    // skipped, not fatal.
    for entry in entries.filter_map(Result::ok) {
        // `read_link` on `/proc/<pid>/fd/<n>` returns the target
        // path; `metadata`/`canonicalize` would dereference and may
        // fail for sockets, pipes, etc. — read_link side-steps that.
        match std::fs::read_link(entry.path()) {
            Ok(link) if link == *target => return true,
            _ => continue,
        }
    }
    false
}

/// Read `/proc/<pid>/stat` and return the pgid (4th field by libc
/// `proc(5)`). The stat line has the form
/// `<pid> (<comm>) <state> <ppid> <pgid> ...`; the comm field can
/// contain spaces and parens, so we split off everything up to the
/// last `)` before tokenizing.
fn read_pgid(proc_root: &Path, pid: i32) -> io::Result<i32> {
    let stat_path = proc_root.join(pid.to_string()).join("stat");
    let raw = std::fs::read_to_string(&stat_path)?;
    let after_comm = raw.rsplit_once(')').map(|(_, rest)| rest).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("malformed /proc/{pid}/stat"),
        )
    })?;
    let mut fields = after_comm.split_whitespace();
    // After ')' the remaining whitespace-tokens are: state ppid pgid ...
    // We want field index 2 (pgid) of the post-comm tail.
    fields.next(); // state
    fields.next(); // ppid
    let pgid_str = fields.next().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("missing pgid field in /proc/{pid}/stat"),
        )
    })?;
    pgid_str.parse::<i32>().map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("pgid parse: {e} in /proc/{pid}/stat"),
        )
    })
}

#[cfg(test)]
mod tests;
