//! The workflow-source seam (ARCH §6 *The workflow mark*,
//! `docs/DESIGN_WORKFLOW_SWITCH.md`): resolution answers the workflow
//! question from the nearest workflow mark on the agent's descent, else
//! the governing config commit — today's path, byte for byte, which the
//! unmarked cases here pin as the **basic agentic loop** default. Real
//! git + a real scaffolded workspace; the adapter is never spawned (the
//! `models.yaml` `adapter:` override skips the load-time guard, §4.4).

use super::workflow_source::nearest_mark;
use super::{ConfigSource, resolve_worker};
use crate::prompt::inbox::Launcher;
use crate::prompt::{Deps, NanoIdGen, SystemClock};
use crate::template::{GitRunner, RealGit};
use crate::workspace::{self, fixture, workflow_mark};
use std::path::Path;
use std::sync::atomic::AtomicBool;
use tempfile::TempDir;

struct NoAdapter;
impl crate::prompt::AdapterRunner for NoAdapter {
    fn run(
        &self,
        _b: &std::ffi::OsString,
        _a: &[&str],
        _s: &[u8],
        _o: &mut dyn FnMut(&[u8]) -> std::io::Result<()>,
    ) -> std::io::Result<Vec<u8>> {
        unreachable!("the adapter is never reached at resolution")
    }
}
struct NoSleeper;
impl crate::prompt::Sleeper for NoSleeper {
    fn sleep(&self, _d: std::time::Duration) {
        unreachable!("the sleeper is never reached at resolution")
    }
}
struct NoTools;
impl crate::prompt::tool::ToolExecutor for NoTools {
    fn execute(
        &self,
        _c: crate::prompt::tool::ToolCall<'_>,
        _s: &Path,
        _st: &AtomicBool,
        _b: Option<crate::config::ToolOutputBound>,
    ) -> Result<crate::prompt::tool::ToolOutcome, crate::prompt::ExecError> {
        unreachable!("the tool executor is never reached at resolution")
    }
}
struct NoLauncher;
impl Launcher for NoLauncher {
    fn launch(&self, _ws: &Path, _agent: &str) -> std::io::Result<()> {
        unreachable!("no driver is launched at resolution")
    }
}

/// Owns the deps components; `models.yaml` names an `adapter:` override
/// so the `bz --version` guard is skipped (§4.4).
struct Fx {
    git: RealGit,
    clock: SystemClock,
    id: NanoIdGen,
    adapter: NoAdapter,
    sleeper: NoSleeper,
    tools: NoTools,
    launcher: NoLauncher,
    stop: AtomicBool,
    cfg: TempDir,
}
impl Fx {
    fn new() -> Self {
        let cfg = TempDir::new().unwrap();
        std::fs::write(cfg.path().join("models.yaml"), "adapter: /bin/true\n").unwrap();
        Self {
            git: RealGit::new(),
            clock: SystemClock,
            id: NanoIdGen,
            adapter: NoAdapter,
            sleeper: NoSleeper,
            tools: NoTools,
            launcher: NoLauncher,
            stop: AtomicBool::new(false),
            cfg,
        }
    }
    fn deps(&self) -> Deps<'_> {
        Deps {
            adapter: &self.adapter,
            sleeper: &self.sleeper,
            git: &self.git,
            clock: &self.clock,
            id_gen: &self.id,
            tool_executor: &self.tools,
            config_root: self.cfg.path(),
            adapter_target: None,
            stop: &self.stop,
            launcher: &self.launcher,
            rng: crate::workspace::agent_name::mint::test_rng(),
        }
    }
}

/// The head of `config/default`.
fn head(ws: &Path) -> String {
    RealGit::new()
        .run_capture(
            &workspace::repo_git(ws),
            &["rev-parse", &workspace::config_ref("default")],
        )
        .unwrap()
        .trim()
        .to_string()
}

/// A `workflow.yaml` whose retry cap identifies it — valid under the
/// closed §6 vocabulary, distinguishable from the shipped default's 3.
const SWITCHED_WORKFLOW: &str = "events: {}\nretry:\n  max_attempts: 7\n  backoff: exponential\n";

#[test]
fn an_unmarked_agent_resolves_the_governing_workflow_the_basic_agentic_loop() {
    // The equivalence pin: no mark → the governing config commit's
    // `workflow.yaml`, which for a template-born workspace is the named
    // **basic agentic loop** default — retry 3, compaction every 20.
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-r1");
    let fx = Fx::new();
    let cfg = resolve_worker(&ws, ConfigSource::Agent("20260101-r1"), &fx.deps()).unwrap();
    assert_eq!(cfg.workflow.retry.max_attempts, 3);
    assert!(cfg.workflow.compaction.is_some());
}

#[test]
fn a_marked_agent_resolves_the_marked_workflow_and_nothing_else_moves() {
    // The switch: the marked commit answers the workflow question alone —
    // the soul (and every other control fact) still resolves from the
    // governing commit, so the mark moves policy, not identity.
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-r1");
    fixture::amend_config(
        &ws,
        &[
            ("workflow.yaml", SWITCHED_WORKFLOW),
            ("souls/worker.md", "a switched soul\n"),
        ],
    );
    let fx = Fx::new();
    workflow_mark::write(&ws, "20260101-r1", &head(&ws), &fx.git).unwrap();
    let cfg = resolve_worker(&ws, ConfigSource::Agent("20260101-r1"), &fx.deps()).unwrap();
    assert_eq!(cfg.workflow.retry.max_attempts, 7, "the marked workflow");
    assert!(
        !cfg.soul.contains("a switched soul"),
        "the soul is still the governing commit's — the mark switches the workflow fact alone",
    );
}

#[test]
fn clearing_the_mark_returns_resolution_to_the_governing_workflow() {
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-r1");
    fixture::amend_config(&ws, &[("workflow.yaml", SWITCHED_WORKFLOW)]);
    let fx = Fx::new();
    workflow_mark::write(&ws, "20260101-r1", &head(&ws), &fx.git).unwrap();
    workflow_mark::clear(&ws, "20260101-r1", &fx.git).unwrap();
    let cfg = resolve_worker(&ws, ConfigSource::Agent("20260101-r1"), &fx.deps()).unwrap();
    assert_eq!(cfg.workflow.retry.max_attempts, 3, "back to the default");
}

#[test]
fn a_child_resolves_the_nearest_ancestor_mark_so_marking_the_root_switches_the_tree() {
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-r1");
    fixture::spawn_agent(&ws, "20260101-r1-20260102-c1", "agents/20260101-r1");
    fixture::amend_config(&ws, &[("workflow.yaml", SWITCHED_WORKFLOW)]);
    let fx = Fx::new();
    workflow_mark::write(&ws, "20260101-r1", &head(&ws), &fx.git).unwrap();
    let cfg = resolve_worker(
        &ws,
        ConfigSource::Agent("20260101-r1-20260102-c1"),
        &fx.deps(),
    )
    .unwrap();
    assert_eq!(cfg.workflow.retry.max_attempts, 7, "inherited by descent");
}

#[test]
fn a_childs_own_mark_overrides_its_ancestors() {
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-r1");
    fixture::spawn_agent(&ws, "20260101-r1-20260102-c1", "agents/20260101-r1");
    let governing = head(&ws);
    fixture::amend_config(&ws, &[("workflow.yaml", SWITCHED_WORKFLOW)]);
    let fx = Fx::new();
    // Root marked at the switched head; the child pinned to the old one.
    workflow_mark::write(&ws, "20260101-r1", &head(&ws), &fx.git).unwrap();
    workflow_mark::write(&ws, "20260101-r1-20260102-c1", &governing, &fx.git).unwrap();
    assert_eq!(
        nearest_mark(&ws, "20260101-r1-20260102-c1", &fx.git),
        Some(governing),
        "nearest wins: the child's own mark, not the root's",
    );
    let cfg = resolve_worker(
        &ws,
        ConfigSource::Agent("20260101-r1-20260102-c1"),
        &fx.deps(),
    )
    .unwrap();
    assert_eq!(cfg.workflow.retry.max_attempts, 3);
}

#[test]
fn a_fresh_fork_has_no_descent_and_resolves_the_governing_workflow() {
    // ConfigSource::Fork — a root about to be forked has no id and so no
    // mark; the governing path answers, exactly as before the seam.
    let (_h, ws) = fixture::workspace();
    let fx = Fx::new();
    let spec = workspace::config_ref("default");
    let cfg = resolve_worker(&ws, ConfigSource::Fork(&spec), &fx.deps()).unwrap();
    assert_eq!(cfg.workflow.retry.max_attempts, 3);
}

#[test]
fn a_marked_commit_failing_the_version_guard_declines_resolution() {
    // §10 discipline holds for the marked commit too: its workflow may
    // carry shapes this harness cannot read, so the guard runs before
    // the parse — declined loudly, not misread.
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-r1");
    fixture::amend_config(
        &ws,
        &[
            ("version", "not-a-version\n"),
            ("workflow.yaml", SWITCHED_WORKFLOW),
        ],
    );
    let fx = Fx::new();
    workflow_mark::write(&ws, "20260101-r1", &head(&ws), &fx.git).unwrap();
    let err = match resolve_worker(&ws, ConfigSource::Agent("20260101-r1"), &fx.deps()) {
        Err(err) => err,
        Ok(_) => panic!("a marked commit with a bad version must decline"),
    };
    assert!(err.to_string().contains("version"), "{err}");
}

#[test]
fn a_marked_commit_whose_workflow_does_not_parse_declines_resolution() {
    // The verb pre-flights this, but a mark is a ref anyone can write:
    // resolution still declines loudly rather than stepping on a policy
    // it cannot read.
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-r1");
    fixture::amend_config(
        &ws,
        &[(
            "workflow.yaml",
            "events:\n  user_message: [not_an_action]\n",
        )],
    );
    let fx = Fx::new();
    workflow_mark::write(&ws, "20260101-r1", &head(&ws), &fx.git).unwrap();
    assert!(resolve_worker(&ws, ConfigSource::Agent("20260101-r1"), &fx.deps()).is_err());
}

#[test]
fn a_mark_at_a_commit_with_no_workflow_declines_as_a_control_read() {
    // A mark aimed at a commit that carries no `workflow.yaml` at all —
    // an agent's own tip, say — is a defective mark; the control read
    // names the missing address instead of silently falling back.
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-r1");
    let fx = Fx::new();
    let orphan = orphan_commit(&ws, &fx.git);
    workflow_mark::write(&ws, "20260101-r1", &orphan, &fx.git).unwrap();
    assert!(resolve_worker(&ws, ConfigSource::Agent("20260101-r1"), &fx.deps()).is_err());
}

/// An empty orphan commit in the workspace repo — a commit-ish carrying
/// none of the control files.
fn orphan_commit(ws: &Path, git: &RealGit) -> String {
    let repo = workspace::repo_git(ws);
    let tree = git
        .run_capture(&repo, &["mktree"])
        .unwrap()
        .trim()
        .to_string();
    git.run_capture(&repo, &["commit-tree", "-m", "empty", &tree])
        .unwrap()
        .trim()
        .to_string()
}
