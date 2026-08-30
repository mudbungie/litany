//! The §2.11 exit race, end to end with real git: a deposit landing
//! between the executor's final drain and its lock release is delivered
//! by the exit-launched driver ([`crate::prompt::dispatch::driver`]).
//! Also the fire-and-forget swallow and the negatives of the
//! [`super::exit_launch`] helpers — split there for the per-file line
//! cap.

use super::exit_launch::{PROBE_RETRIES, deposit_files, probe_until_free};
use super::fixtures::*;
use crate::prompt::adapter::AdapterRunner;
use crate::prompt::dispatch::driver::{self, DriveOutcome};
use crate::prompt::inbox::{Launcher, inbox_dir, try_acquire};
use crate::prompt::{Deps, run};
use crate::template::{GitRunner, RealGit};
use crate::workspace::agent_name::mint::test_rng;
use std::cell::RefCell;
use std::ffi::OsString;
use std::io;
use std::path::Path;

#[test]
fn probe_until_free_reports_a_genuinely_held_lock() {
    // A lock held for the whole probe window is a real contender, not
    // the fork-inheritance blip — the probe gives up and says so.
    let ws = tempfile::TempDir::new().unwrap();
    let _held = try_acquire(&inbox_dir(ws.path(), "a1")).unwrap().unwrap();
    assert!(!probe_until_free(ws.path(), "a1").unwrap());
}

#[test]
fn deposit_files_walks_only_what_is_legible() {
    let ws = tempfile::TempDir::new().unwrap();
    // No inbox root at all → nothing.
    assert!(deposit_files(ws.path()).is_empty());
    // A stray file where an agent dir should be is skipped; a real
    // deposit is read.
    std::fs::create_dir_all(ws.path().join("inbox").join("a1")).unwrap();
    std::fs::write(ws.path().join("inbox").join("stray"), b"x").unwrap();
    std::fs::write(
        ws.path().join("inbox").join("a1").join("user-001.md"),
        b"hi",
    )
    .unwrap();
    assert_eq!(deposit_files(ws.path()), vec!["hi".to_string()]);
}

/// A launcher that cannot spawn: the §2.11 fire-and-forget contract says
/// the failure is logged and swallowed, never propagated.
struct FailingLauncher;
impl Launcher for FailingLauncher {
    fn launch(&self, _workspace: &Path, _agent_id: &str) -> io::Result<()> {
        Err(io::Error::other("spawn refused"))
    }
}

#[test]
fn a_failing_exit_launch_is_swallowed() {
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let harness = scaffold_harness_root();
    let adapter = StubAdapter::happy(&stream_of(brazen::FinishReason::Stop, &[Block::Text("hi")]));
    let git = StubGit::ok();
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let (sleeper, tool_executor) = (StubSleeper::default(), StubToolExecutor::ok());

    let mut deps = valid_deps(
        &adapter,
        &sleeper,
        &git,
        &clock,
        &id,
        &tool_executor,
        harness.path(),
    );
    deps.launcher = &FailingLauncher;

    // The exchange still succeeds: the launch is fire-and-forget.
    run(
        repo.path(),
        "go",
        None,
        None,
        None,
        crate::prompt::PinnedDocs::none(),
        None,
        &deps,
    )
    .unwrap();
}

/// Adapter for the exit-race test: the version-guard probe replies
/// normally; the model call first deposits into the exiting agent's own
/// inbox — *after* this step's drain, i.e. inside the crack the exit
/// protocol closes — then streams a final response.
struct DepositMidCall<'a> {
    workspace: &'a Path,
    agent: &'a str,
}

impl AdapterRunner for DepositMidCall<'_> {
    fn run(
        &self,
        _binary: &OsString,
        args: &[&str],
        _stdin: &[u8],
        on_line: &mut dyn FnMut(&[u8]) -> io::Result<()>,
    ) -> io::Result<Vec<u8>> {
        let bytes = if args.contains(&"--version") {
            version_line()
        } else {
            crate::prompt::inbox::deposit(
                self.workspace,
                self.agent,
                "user",
                "late mail",
                &crate::prompt::SystemClock,
            )
            .map_err(io::Error::other)?;
            stream_of(brazen::FinishReason::Stop, &[Block::Text("done")])
        };
        for line in bytes.split(|b| *b == b'\n').filter(|l| !l.is_empty()) {
            on_line(line)?;
        }
        Ok(Vec::new())
    }
}

/// The exit-launched driver, run in-process: what `litany advance` will
/// do on arrival ([`driver::drive`]), with the outcome recorded.
struct DriveLauncher {
    outcomes: RefCell<Vec<DriveOutcome>>,
}

impl Launcher for DriveLauncher {
    fn launch(&self, workspace: &Path, agent_id: &str) -> io::Result<()> {
        // Retry across the fork→exec fd-inheritance window (see
        // `probe_until_free`): a real contender would hold forever.
        for _ in 0..PROBE_RETRIES {
            let outcome =
                driver::drive(workspace, agent_id, &RealGit::new()).map_err(io::Error::other)?;
            if outcome != DriveOutcome::AlreadyDriven {
                self.outcomes.borrow_mut().push(outcome);
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        self.outcomes.borrow_mut().push(DriveOutcome::AlreadyDriven);
        Ok(())
    }
}

#[test]
fn drive_launcher_reports_a_genuinely_held_lock_as_already_driven() {
    // Same give-up shape as `probe_until_free`: a lock held for the whole
    // window is a real executor, and the launched driver's clean no-op
    // (Writer/driver totality, §2.11) is the recorded outcome.
    let ws = tempfile::TempDir::new().unwrap();
    let _held = try_acquire(&inbox_dir(ws.path(), "a1")).unwrap().unwrap();
    let launcher = DriveLauncher {
        outcomes: RefCell::new(Vec::new()),
    };
    launcher.launch(ws.path(), "a1").unwrap();
    assert_eq!(
        *launcher.outcomes.borrow(),
        vec![DriveOutcome::AlreadyDriven]
    );
}

#[test]
fn exit_race_late_deposit_is_delivered_via_the_exit_launched_driver() {
    // Real git end to end: a deposit lands between the final drain and
    // the lock release; the unconditional exit launch hands the branch to
    // a driver that acquires the freed lock and delivers it (§2.11).
    let (_holder, repo) = crate::workspace::fixture::workspace();
    crate::workspace::fixture::amend_config(
        &repo,
        &[
            ("providers.yaml", VALID_PER_REPO_PROVIDERS_YAML),
            ("workflow.yaml", VALID_WORKFLOW_YAML),
            ("souls/worker.md", "soul"),
        ],
    );
    let harness = scaffold_harness_root();

    let agent = "ct-1-deadbeef";
    let adapter = DepositMidCall {
        workspace: &repo,
        agent,
    };
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let (sleeper, tool_executor) = (StubSleeper::default(), StubToolExecutor::ok());
    let launcher = DriveLauncher {
        outcomes: RefCell::new(Vec::new()),
    };
    let deps = Deps {
        adapter: &adapter,
        sleeper: &sleeper,
        git: &RealGit::new(),
        clock: &clock,
        id_gen: &id,
        tool_executor: &tool_executor,
        config_root: harness.path(),
        adapter_target: None,
        stop: never_stopped(),
        launcher: &launcher,
        rng: test_rng(),
    };

    let branch = run(
        &repo,
        "go",
        None,
        None,
        None,
        crate::prompt::PinnedDocs::none(),
        None,
        &deps,
    )
    .unwrap();
    assert_eq!(branch, agent);
    // Two launches, in sequence (§2.11): the release rule's — the late
    // deposit is outside what the last drain deliberately left pending,
    // so the tail's post-release re-read fires, and that driver wins the
    // freed lock and delivers — then the exit protocol's unconditional
    // self-launch, whose driver finds the inbox quiet again (racy
    // launches are free).
    assert_eq!(
        *launcher.outcomes.borrow(),
        vec![DriveOutcome::Delivered(1), DriveOutcome::NothingToDeliver]
    );
    // Delivered means committed: the transcript on the branch carries it
    // (001-user = initial message, 002-<model-id> = assistant, 003-user =
    // the late deposit's delivery commit).
    let shown = RealGit::new()
        .run_capture(
            &crate::workspace::repo_git(&repo),
            &[
                "show",
                &format!(
                    "{}:messages/003-user.md",
                    crate::workspace::agent_ref(agent)
                ),
            ],
        )
        .unwrap();
    assert!(shown.contains("late mail"), "got {shown:?}");
    // And the inbox is empty again — the file had one home at every
    // instant (§2.11 rename delivery).
    assert_eq!(
        std::fs::read_dir(inbox_dir(&repo, agent)).unwrap().count(),
        0
    );
}
