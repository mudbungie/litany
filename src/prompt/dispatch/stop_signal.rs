//! Executor-side SIGTERM catch (ARCH §2.9 step 3).
//!
//! When a stop is issued the whole harness process group is signalled —
//! `litany stop` does `kill(-pgid, SIGTERM)` (`stop::cascade`), so the
//! provider adapter (`bz`) and every tool subprocess receive their *own*
//! SIGTERM delivery and die at once, `bz` installing no handler so its
//! `response.json` closes without a trailing `end` (the §2.9 stop
//! signature, §3.5). The executor catches its own copy of that same
//! group signal here: catching it shields nobody (the kernel already
//! delivered to each group member), it only lets the executor deposit its
//! branch's **result message** with a `stopped` epitaph on its way out
//! (§2.6, §2.3 step 5, "Return is not a verb") instead of dying on the
//! spot.
//!
//! **Async-signal-safety.** The handler does one thing — a single atomic
//! store (on POSIX's async-signal-safe list). The deposit runs *outside*
//! the handler, at the step loop's ordinary check points and once more on
//! the way out ([`super::run_exchange`]); §2.9's contract is "on its way
//! out", not "inside the handler". Mirrors the flag-and-poll shape the
//! `bash` built-in already uses (`tool::builtin::bash`) — a minimal
//! `libc::signal` registration, no new dependency.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

/// Process-wide SIGTERM flag. Set by [`on_sigterm`]; read (through the
/// injected [`Deps::stop`]) at the [`super::run_exchange`] check points.
static SIGTERM_FLAG: AtomicBool = AtomicBool::new(false);
/// Guards a single [`libc::signal`] registration per process.
static HANDLER_INSTALLED: OnceLock<()> = OnceLock::new();

/// The signal handler: a lone async-signal-safe atomic store. The step
/// loop observes it on its next check-point tick.
extern "C" fn on_sigterm(_signo: libc::c_int) {
    SIGTERM_FLAG.store(true, Ordering::SeqCst);
}

/// Install [`on_sigterm`] for `SIGTERM`, once per process (`litany
/// prompt` at top-of-main, beside `stop::become_pgid_leader`). Idempotent
/// — a second call is a no-op.
pub fn install() {
    HANDLER_INSTALLED.get_or_init(|| {
        // SAFETY: `on_sigterm` only stores to an `AtomicBool`
        // (async-signal-safe); `libc::signal` is the documented POSIX
        // handler-install call. Same construction as the `bash` built-in.
        unsafe {
            libc::signal(libc::SIGTERM, on_sigterm as *const () as libc::sighandler_t);
        }
    });
}

/// The process-wide flag, for the production [`Deps::stop`] wiring in the
/// `litany prompt` bin. Tests inject their own flag instead.
pub fn flag() -> &'static AtomicBool {
    &SIGTERM_FLAG
}

/// Whether a stop has been requested — the injected flag observed at a
/// check point (§2.9 step 3). Callers pass [`Deps::stop`](super::Deps),
/// so every check point is exercised deterministically with a
/// constructed flag, never the process-wide static, in tests.
pub(super) fn stopped(flag: &AtomicBool) -> bool {
    flag.load(Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::*;

    // All manipulation of the process-wide `SIGTERM_FLAG` lives in this
    // one test so the module's static is never mutated from two test
    // threads at once (the `run_exchange` stop tests read an *injected*
    // flag, never this static).
    #[test]
    fn handler_flag_and_real_signal() {
        // Direct handler call: the async-signal-safe store flips the flag.
        SIGTERM_FLAG.store(false, Ordering::SeqCst);
        assert!(!SIGTERM_FLAG.load(Ordering::SeqCst));
        on_sigterm(libc::SIGTERM);
        assert!(SIGTERM_FLAG.load(Ordering::SeqCst));

        // `flag()` hands back that very static.
        assert!(flag().load(Ordering::SeqCst));

        // Install the real handler and prove the OS-signal path: raising
        // SIGTERM at ourselves runs `on_sigterm` (installed, so the
        // default terminate disposition is replaced) and sets the flag.
        SIGTERM_FLAG.store(false, Ordering::SeqCst);
        install();
        install(); // idempotent — OnceLock swallows the second call.
        // SAFETY: `raise` delivers SIGTERM to this thread, where the
        // installed handler catches it; the process is not terminated.
        unsafe {
            libc::raise(libc::SIGTERM);
        }
        assert!(SIGTERM_FLAG.load(Ordering::SeqCst));
        SIGTERM_FLAG.store(false, Ordering::SeqCst);
    }
}
