//! SIGTERM-then-SIGKILL signal sequence for `litany stop`.
//!
//! Mirrors the same pattern §3.3 (tools) and §4.4 (provider adapters)
//! pin: send SIGTERM, wait the deadline polling for the process group
//! to drain, then SIGKILL anything that survived. The signal target
//! is the pgid (negative pid argument to `kill(2)`) so the kernel
//! cascades through every process in the harness's group — adapter
//! subprocesses, dispatched subagent harnesses, cooperating tool
//! subprocesses (§2.9).
//!
//! [`Signaler`] is the swappable surface: production uses
//! [`RealSignaler`] which dispatches to libc::kill; tests use a
//! recording stub that captures `(target, signo)` pairs without
//! actually signalling anything. Same reason [`super::PgidFinder`]
//! is `&dyn`-shaped — a stop test must never accidentally kill the
//! cargo test harness.

use std::thread;
use std::time::{Duration, Instant};

/// Send-signal abstraction over `libc::kill`. The pgid is passed
/// directly (already negative-encoded by the caller? no — the caller
/// passes the positive pgid and the impl negates inside `term`/`kill`
/// before handing to `libc::kill`). `alive` polls one process by pid
/// (`kill(pid, 0)` is the canonical "is this pid still alive" probe)
/// and is used during the SIGTERM/SIGKILL grace.
pub trait Signaler {
    fn term(&self, pgid: i32);
    fn kill(&self, pgid: i32);
    /// Returns true while the leader pid (== pgid for a pgid leader)
    /// is alive in the kernel's process table.
    fn alive(&self, pid: i32) -> bool;
}

/// Production [`Signaler`] backed by libc::kill.
#[derive(Debug, Default, Clone, Copy)]
pub struct RealSignaler;

impl Signaler for RealSignaler {
    fn term(&self, pgid: i32) {
        // SAFETY: signo is a constant; negative pid addresses the
        // process group, which is the documented `kill(2)` calling
        // convention. Failure (ESRCH on a vanished group) is benign
        // — the cascade caller already polls `alive` and treats a
        // missing process as success.
        unsafe {
            libc::kill(-pgid, libc::SIGTERM);
        }
    }
    fn kill(&self, pgid: i32) {
        // SAFETY: see above; SIGKILL is uncatchable so the kernel
        // reaps the group before the caller's next poll.
        unsafe {
            libc::kill(-pgid, libc::SIGKILL);
        }
    }
    fn alive(&self, pid: i32) -> bool {
        // SAFETY: signo `0` performs the kernel's permission /
        // existence check without actually delivering a signal —
        // the canonical `kill(pid, 0)` aliveness probe.
        let r = unsafe { libc::kill(pid, 0) };
        if r == 0 {
            return true;
        }
        // ESRCH means "no such process". EPERM means "the process
        // exists but you can't signal it" — for our purposes
        // (waiting for our own setpgid'd harness to die) that
        // shouldn't happen, but treat it as "still alive" so we
        // don't prematurely declare success.
        let errno = std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or(libc::ESRCH);
        errno != libc::ESRCH
    }
}

/// SIGTERM each pgid, wait up to `deadline` polling for them to
/// exit, then SIGKILL anything still alive. Returns when every pgid
/// has been signalled and either exited or been SIGKILL'd. The
/// caller-supplied `signaler` lets tests assert on the call sequence
/// without a real cascade.
pub fn cascade(pgids: &[i32], signaler: &dyn Signaler, deadline: Duration, poll: Duration) {
    for pgid in pgids {
        signaler.term(*pgid);
    }
    let term_until = Instant::now() + deadline;
    while Instant::now() < term_until {
        if pgids.iter().all(|pgid| !signaler.alive(*pgid)) {
            return;
        }
        thread::sleep(poll);
    }
    for pgid in pgids {
        if signaler.alive(*pgid) {
            signaler.kill(*pgid);
        }
    }
}

/// Recording stub used by both this module's tests and
/// [`super::tests`]. Captures every `(signo, target)` pair the
/// cascade emits and exposes a hook for "process drains after N
/// term polls" so deadline expiry is exercisable without sleeping
/// the wall-clock 5 seconds.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct RecordingSignaler {
    pub(crate) invocations: std::sync::Mutex<Vec<(&'static str, i32)>>,
    pub(crate) alive_polls_remaining: std::sync::atomic::AtomicI32,
}

#[cfg(test)]
impl RecordingSignaler {
    pub(crate) fn new(alive_polls: i32) -> Self {
        Self {
            invocations: std::sync::Mutex::new(Vec::new()),
            alive_polls_remaining: std::sync::atomic::AtomicI32::new(alive_polls),
        }
    }
    pub(crate) fn took(&self) -> Vec<(&'static str, i32)> {
        self.invocations.lock().unwrap().clone()
    }
}

#[cfg(test)]
impl Signaler for RecordingSignaler {
    fn term(&self, pgid: i32) {
        self.invocations.lock().unwrap().push(("term", pgid));
    }
    fn kill(&self, pgid: i32) {
        self.invocations.lock().unwrap().push(("kill", pgid));
    }
    fn alive(&self, _pid: i32) -> bool {
        let prev = self
            .alive_polls_remaining
            .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        prev > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cascade_sigterm_drains_within_deadline_no_sigkill() {
        // 1 alive poll → cascade asks once, sees dead, returns.
        let s = RecordingSignaler::new(0);
        cascade(&[42], &s, Duration::from_secs(60), Duration::from_millis(1));
        let invocations = s.took();
        // SIGTERM only — no SIGKILL because it drained.
        assert_eq!(invocations, vec![("term", 42)]);
    }

    #[test]
    fn cascade_sigkill_after_deadline_when_still_alive() {
        // alive forever → deadline expires → SIGKILL fires.
        let s = RecordingSignaler::new(i32::MAX);
        cascade(
            &[42],
            &s,
            Duration::from_millis(20),
            Duration::from_millis(1),
        );
        let invocations = s.took();
        assert_eq!(invocations.first(), Some(&("term", 42)));
        assert!(invocations.iter().any(|&(sig, _)| sig == "kill"));
        let kill_count = invocations
            .iter()
            .filter(|&&(sig, _)| sig == "kill")
            .count();
        assert_eq!(kill_count, 1);
    }

    #[test]
    fn cascade_signals_every_pgid_in_term_phase() {
        let s = RecordingSignaler::new(0);
        cascade(
            &[1, 2, 3],
            &s,
            Duration::from_secs(60),
            Duration::from_millis(1),
        );
        let invocations = s.took();
        let term_targets: Vec<i32> = invocations
            .iter()
            .filter(|(sig, _)| *sig == "term")
            .map(|(_, pgid)| *pgid)
            .collect();
        assert_eq!(term_targets, vec![1, 2, 3]);
    }

    /// Spawn a long-running child in its own pgid and reap it to
    /// completion. Returns its pid for `alive` probes; the caller
    /// reaps via `child.wait()` AFTER probing because `alive`
    /// (== `kill(pid, 0)`) returns true for a zombie process —
    /// the kernel keeps the entry until the parent waits.
    fn spawn_in_own_pgid(args: &[&str], stdout: std::process::Stdio) -> std::process::Child {
        use std::os::unix::process::CommandExt as _;
        use std::process::{Command, Stdio};
        let (program, rest) = args.split_first().expect("non-empty argv");
        unsafe {
            Command::new(program)
                .args(rest)
                .stdin(Stdio::null())
                .stdout(stdout)
                .stderr(Stdio::null())
                .pre_exec(|| {
                    libc::setpgid(0, 0);
                    Ok(())
                })
                .spawn()
                .expect("spawn child")
        }
    }

    #[test]
    fn real_signaler_term_kills_then_alive_reports_dead_after_reap() {
        let mut child = spawn_in_own_pgid(&["sleep", "60"], std::process::Stdio::null());
        let pid = child.id() as i32;
        let s = RealSignaler;
        assert!(
            s.alive(pid),
            "child should be alive immediately after spawn"
        );
        s.term(pid);
        // Reap so the kernel drops the zombie's process table entry;
        // alive() (`kill(pid, 0)`) treats a zombie as alive otherwise.
        child.wait().unwrap();
        assert!(!s.alive(pid), "alive should report dead after reap");
    }

    #[test]
    fn real_signaler_kill_uncatchable_takes_down_sigterm_handler() {
        use std::io::BufRead as _;
        let mut child = spawn_in_own_pgid(
            &[
                "sh",
                "-c",
                "trap '' TERM; echo ready; while :; do sleep 1; done",
            ],
            std::process::Stdio::piped(),
        );
        let pid = child.id() as i32;
        // The trap install is observable, not timed: the shell echoes
        // only after `trap` has run, so the SIGTERM below can never
        // race the default disposition. (A timed flip — term, sleep,
        // assert alive — lost that race under load: the signal could
        // land before the trap and fell the fixture.) With the trap
        // provably in place, TERM structurally cannot kill the child,
        // so no settle-wait is needed before the aliveness assert.
        let mut out = std::io::BufReader::new(child.stdout.take().expect("stdout is piped"));
        let mut line = String::new();
        out.read_line(&mut line).expect("read fixture handshake");
        assert_eq!(line.trim(), "ready", "fixture reached its post-trap echo");
        let s = RealSignaler;
        s.term(pid);
        assert!(s.alive(pid), "TERM is trapped; child should still be alive");
        s.kill(pid);
        child.wait().unwrap();
        assert!(!s.alive(pid), "alive should report dead after reap");
    }

    #[test]
    fn real_signaler_alive_returns_false_for_nonexistent_pid() {
        let s = RealSignaler;
        // pid_max is much smaller than i32::MAX; this pid cannot
        // exist in the kernel's process table.
        assert!(!s.alive(i32::MAX), "synthetic pid should be reported dead");
    }

    #[test]
    fn cascade_only_kills_still_alive_pgids() {
        // A single pgid keeps the poll ordering deterministic (`all`
        // short-circuits, so multiple pgids would not exhaust the budget
        // evenly). The deadline is generous — like the sibling
        // drain-tests' `from_secs(60)` — so the *poll-count* path decides
        // the outcome, never the wall clock: `new(2)` guarantees `alive`
        // reports dead on the third poll, so cascade returns via the
        // all-dead branch in ~2ms and never approaches the deadline. (A
        // tight 5ms deadline coupled to the 2-poll drain raced under the
        // coverage runner's instrumentation slowdown, spuriously firing
        // SIGKILL.)
        let s = RecordingSignaler::new(2);
        cascade(&[42], &s, Duration::from_secs(60), Duration::from_millis(1));
        let invocations = s.took();
        let kills: Vec<&(&'static str, i32)> = invocations
            .iter()
            .filter(|(sig, _)| *sig == "kill")
            .collect();
        // alive returns true for 2 polls (loop sleeps), then false →
        // cascade returns without firing SIGKILL.
        assert!(kills.is_empty());
    }
}
