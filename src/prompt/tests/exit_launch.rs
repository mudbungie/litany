//! The §2.11 exit protocol at the step loop's terminal seam: deposit →
//! release own lock → spawn a driver at own agent *and* at the parent
//! the deposit revived → exit. Pin 2 (launch by epitaph value: final
//! response launches, `stopped` and `budget-exhausted` never — at the
//! parent as much as at the exiting agent), the post-release
//! no-authority ordering (the launcher observes a free lock and an
//! already-landed deposit), and the parentless case (deposit no-ops,
//! self-launch still fires, nothing is revived). The child-path
//! revival — a real `litany advance` child terminal waking a real
//! parent — is [`super::parent_revival`]. The
//! fire-and-forget swallow, the helper negatives, and the real-git exit
//! race live in [`super::exit_race`], and the never-launch epitaphs
//! (`stopped`, `budget-exhausted`, the errored executor) in
//! [`super::exit_launch_never`] — split for the per-file line cap.

use super::fixtures::*;
use crate::prompt::inbox::{Launcher, inbox_dir, try_acquire};
use crate::prompt::{Clock, Deps, run};
use crate::workspace::agent_name::mint::test_rng;
use std::cell::RefCell;
use std::io;
use std::path::{Path, PathBuf};

/// Records each launch with what the §2.11 ordering guarantees at that
/// instant: the executor lock already released (a probe succeeds) and
/// the terminal deposit already landed (the parent inbox holds it).
#[derive(Default)]
pub(super) struct ProbingLauncher {
    pub(super) invocations: RefCell<Vec<(PathBuf, String, bool, bool)>>,
}

impl Launcher for ProbingLauncher {
    fn launch(&self, workspace: &Path, agent_id: &str) -> io::Result<()> {
        let lock_free = probe_until_free(workspace, agent_id)?;
        let deposited = deposit_files(workspace)
            .iter()
            .any(|n| n.contains("epitaph"));
        self.invocations.borrow_mut().push((
            workspace.to_path_buf(),
            agent_id.to_string(),
            lock_free,
            deposited,
        ));
        Ok(())
    }
}

/// Bounded retries for every executor-lock probe in these tests — here,
/// in [`super::exit_race`], and in [`super::advance`]. The §2.11 ordering
/// under test released the fd before launching, but a
/// concurrent test thread's `Command` spawn can fork while that fd was
/// still open and hold the inherited duplicate for the fork→exec window
/// (all fds are CLOEXEC, so exec drops it microseconds later). A genuine
/// ordering bug holds the lock forever and still fails; the fork window
/// clears in a retry or two.
///
/// A count, not a duration: the budget must not shrink because the
/// machine is busy, and the give-up arm must be reached by the same
/// number of iterations on every run — a wall-clock deadline makes the
/// retry sleep's coverage load-dependent (see [`super::advance::free_within`]).
pub(super) const PROBE_RETRIES: u32 = 60;

/// Probe the executor lock with the bounded retry above.
pub(super) fn probe_until_free(workspace: &Path, agent_id: &str) -> io::Result<bool> {
    for _ in 0..PROBE_RETRIES {
        if try_acquire(&inbox_dir(workspace, agent_id))?.is_some() {
            return Ok(true);
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    Ok(false)
}

/// Every deposited file body under `<workspace>/inbox/**` (flat walk).
pub(super) fn deposit_files(workspace: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(agents) = std::fs::read_dir(workspace.join("inbox")) else {
        return out;
    };
    for agent in agents.flatten() {
        let Ok(rd) = std::fs::read_dir(agent.path()) else {
            continue;
        };
        for f in rd.flatten() {
            out.push(std::fs::read_to_string(f.path()).unwrap_or_default());
        }
    }
    out
}

/// [`run`] as every start in this file issues it — a plain root start
/// (no fork point, no name, no pins); what varies here is only the
/// §2.11 exit-launch behaviour under test.
pub(super) fn plain_run(repo: &Path, deps: &Deps<'_>) -> Result<String, crate::prompt::Error> {
    run(
        repo,
        "go",
        None,
        None,
        None,
        crate::prompt::PinnedDocs::none(),
        None,
        deps,
    )
}

#[test]
fn a_user_prompted_final_response_launches_own_agent_and_revives_nobody() {
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let harness = scaffold_harness_root();
    let adapter = StubAdapter::happy(&stream_of(brazen::FinishReason::Stop, &[Block::Text("hi")]));
    let git = StubGit::ok();
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let (sleeper, tool_executor) = (StubSleeper::default(), StubToolExecutor::ok());
    let launcher = ProbingLauncher::default();

    let mut deps = valid_deps(
        &adapter,
        &sleeper,
        &git,
        &clock,
        &id,
        &tool_executor,
        harness.path(),
    );
    deps.launcher = &launcher;

    plain_run(repo.path(), &deps).unwrap();
    let invocations = launcher.invocations.borrow();
    // `litany prompt` deposits the operator's message into the agent's
    // own inbox and the step-1 drain delivers it (§2.11), so the last
    // prompter is `user` — the reply is read in this agent's own
    // conversation and addresses no inbox (§2.6). This id has a
    // derivable dispatcher (`ct`), and it still gets nothing: the
    // address is the transcript's answer, not the id's.
    assert_eq!(invocations.len(), 1, "no recipient, so no revival");
    let (ws, agent, lock_free, deposited) = &invocations[0];
    assert_eq!(ws, repo.path());
    assert_eq!(agent, "ct-1-deadbeef");
    // §2.11 ordering: release → launch (the self-directed launch is
    // unconditional on a final response, whoever the reply addressed).
    assert!(*lock_free, "the lock must be released before the launch");
    assert!(!*deposited, "an operator-prompted reply deposits nothing");
    assert!(!inbox_dir(repo.path(), "ct").exists(), "no parent inbox");
}

/// A hyphen-free compact stamp makes the conv-id a two-token *root*
/// (`parent_of` = None): the parentless arm of the terminal sequence.
struct RootClock;
impl Clock for RootClock {
    fn now_iso8601(&self) -> String {
        "iso".into()
    }
    fn now_compact(&self) -> String {
        "ct1".into()
    }
}

#[test]
fn parentless_agent_deposit_noops_but_exit_launch_still_fires() {
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let harness = scaffold_harness_root();
    let adapter = StubAdapter::happy(&stream_of(brazen::FinishReason::Stop, &[Block::Text("hi")]));
    let git = StubGit::ok();
    let (clock, id) = (RootClock, FixedIdGen);
    let (sleeper, tool_executor) = (StubSleeper::default(), StubToolExecutor::ok());
    let launcher = ProbingLauncher::default();
    let deps = Deps {
        adapter: &adapter,
        sleeper: &sleeper,
        git: &git,
        clock: &clock,
        id_gen: &id,
        tool_executor: &tool_executor,
        config_root: harness.path(),
        adapter_target: None,
        stop: never_stopped(),
        launcher: &launcher,
        rng: test_rng(),
    };

    let branch = plain_run(repo.path(), &deps).unwrap();
    assert_eq!(branch, "ct1-deadbeef", "two tokens: a parentless root");
    // The deposit is a structural no-op — no result message anywhere…
    assert!(
        !deposit_files(repo.path())
            .iter()
            .any(|b| b.contains("epitaph")),
        "a root deposits no result"
    );
    // …and the one unconditional sequence still launches (no agent kinds).
    let invocations = launcher.invocations.borrow();
    assert_eq!(invocations.len(), 1);
    assert_eq!(invocations[0].1, "ct1-deadbeef");
}
