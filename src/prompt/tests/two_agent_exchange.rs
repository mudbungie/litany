//! **One exchange between two agents terminates** (ARCH §2.6 *A reply is
//! owed once*, bl-82d8), driven end to end on the real child path.
//!
//! Two siblings that had cooperated — one wrote a spec and messaged the
//! other, the other did the work and messaged back — then never stopped:
//! each one's terminal response was deposited into the other's inbox,
//! woke it, and produced a terminal response deposited back, at the model
//! spend of a real step apiece. The `message` tool was called once in 333
//! transcript rows.
//!
//! The floor is the reply debt: **being answered asks nothing**, so a
//! terminal event whose window carries only a peer's reply owes nobody
//! and deposits nothing. This file runs the exchange with a launcher that
//! *is* the launched driver — every launch runs that agent's `litany
//! advance` hop in-process, recursively, exactly as the detached spawn
//! would — under a hard hop cap. Termination is the assertion the cap
//! cannot fake: on the pre-bl-82d8 tree the run reaches the cap.

use super::advance::{RecLauncher, worker_config};
use super::fixtures::*;
use super::parent_revival::DescentClock;
use crate::prompt::dispatch::advance::run;
use crate::prompt::inbox::{self, Launcher, inbox_dir};
use crate::prompt::{Clock, PinnedDocs};
use crate::template::RealGit;
use crate::workspace::agent_name::mint::test_rng;
use crate::workspace::fixture;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::{io, sync::atomic::AtomicUsize, sync::atomic::Ordering};

/// The exchange must settle in far fewer hops than this. It is a
/// tripwire, not a tuning knob: an unbounded exchange trips it instead of
/// hanging the suite, and a bounded one never reaches it.
const HOP_CAP: usize = 12;

/// A second descent-shaped stamp, so the two siblings mint distinct ids
/// off the same parent (§2.3).
struct SecondClock;
impl Clock for SecondClock {
    fn now_iso8601(&self) -> String {
        "iso".into()
    }
    fn now_compact(&self) -> String {
        "ct2".into()
    }
}

/// A launcher that *is* every launched driver: it records the launch and
/// runs that agent's hop in-process with itself as the launcher, so the
/// exchange runs to its own fixed point. Past [`HOP_CAP`] it refuses to
/// launch, which is how a non-terminating exchange becomes a failed
/// assertion rather than an infinite test.
struct ExchangeLauncher {
    hops: AtomicUsize,
    invocations: RefCell<Vec<String>>,
}

impl ExchangeLauncher {
    fn new() -> Self {
        Self {
            hops: AtomicUsize::new(0),
            invocations: RefCell::new(Vec::new()),
        }
    }
}

impl Launcher for ExchangeLauncher {
    fn launch(&self, ws: &Path, agent: &str) -> io::Result<()> {
        self.invocations.borrow_mut().push(agent.to_string());
        if self.hops.fetch_add(1, Ordering::SeqCst) < HOP_CAP {
            advance_with(ws, agent, self);
        }
        Ok(())
    }
}

/// One `litany advance` hop at `agent`, with `launcher` wired in as the
/// §2.11 launch seam — the in-process stand-in for the detached spawn.
fn advance_with(ws: &Path, agent: &str, launcher: &dyn Launcher) {
    let adapter = StubAdapter::scripted([StubAdapter::reply_ok(&happy_response_bytes())]);
    let (sleeper, tools, stub_git) = (
        StubSleeper::default(),
        StubToolExecutor::ok(),
        StubGit::ok(),
    );
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let git = RealGit::new();
    let mut deps = valid_deps(&adapter, &sleeper, &stub_git, &clock, &id, &tools, ws);
    deps.git = &git;
    deps.launcher = launcher;
    run(ws, agent, None, &deps, &mut || Ok(worker_config())).unwrap();
}

/// A root with two dispatched siblings, each already advanced once — so
/// each has answered its own dispatch and carries a terminal response,
/// which is the state a cooperating pair is in when the exchange starts.
fn two_siblings() -> (tempfile::TempDir, PathBuf, String, String) {
    use crate::prompt::child_dispatch::{ChildDispatchRequest, run as dispatch_child};
    let (holder, ws) = fixture::workspace();
    let parent = "20260101-a1";
    let parent_wt = fixture::spawn_root(&ws, parent);
    let child = |clock: &dyn Clock| {
        dispatch_child(
            &ChildDispatchRequest {
                repo: &ws,
                parent_branch: parent,
                parent_worktree: &parent_wt,
                role: "worker",
                goal: "cooperate",
                name: None,
                fork_point: None,
                cwd: None,
                pins: PinnedDocs::none(),
            },
            &RealGit::new(),
            clock,
            &FixedIdGen,
            no_launch(),
            test_rng(),
        )
        .unwrap()
    };
    let speccer = child(&DescentClock);
    let builder = child(&SecondClock);
    // Each answers its own dispatch: one hop, one terminal response, one
    // reply into the root's inbox. The inert launcher stops there.
    let inert = RecLauncher::default();
    advance_with(&ws, &speccer, &inert);
    advance_with(&ws, &builder, &inert);
    (holder, ws, speccer, builder)
}

/// Every result message (§2.6) pending in `agent`'s inbox.
fn pending_results(ws: &Path, agent: &str) -> Vec<String> {
    std::fs::read_dir(inbox_dir(ws, agent))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| std::fs::read_to_string(e.path()).unwrap())
        .filter(|b| b.contains("terminal_ref:"))
        .collect()
}

#[test]
fn one_exchange_between_two_agents_terminates() {
    let (_holder, ws, speccer, builder) = two_siblings();
    // The exchange, as the `message` tool deposits it (§2.11): the spec
    // to the builder, and the builder's "DONE" back — one call each, the
    // whole of what the two agents deliberately said to each other.
    inbox::deposit(&ws, &builder, &speccer, "the spec", &DescentClock).unwrap();
    inbox::deposit(&ws, &speccer, &builder, "DONE", &DescentClock).unwrap();

    let launcher = ExchangeLauncher::new();
    advance_with(&ws, &builder, &launcher);

    // The pre-bl-82d8 tree runs until the cap: each terminal response is
    // deposited back into the peer, which wakes and answers it.
    let hops = launcher.hops.load(Ordering::SeqCst);
    assert!(
        hops < HOP_CAP,
        "the exchange did not terminate — {hops} hops: {:?}",
        launcher.invocations.borrow()
    );
    // And it ends quiet: no reply is left waiting in either inbox for a
    // later touch to deliver, so nothing revives either agent.
    assert!(
        pending_results(&ws, &speccer).is_empty() && pending_results(&ws, &builder).is_empty(),
        "an exchange that settled leaves no reply behind: {:?} / {:?}",
        pending_results(&ws, &speccer),
        pending_results(&ws, &builder)
    );
}
