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
pub(super) struct Fx {
    pub(super) git: RealGit,
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
    pub(super) fn new() -> Self {
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
    pub(super) fn deps(&self) -> Deps<'_> {
        Deps {
            adapter: &self.adapter,
            sleeper: &self.sleeper,
            git: &self.git,
            clock: &self.clock,
            id_gen: &self.id,
            tool_executor: &self.tools,
            config_root: self.cfg.path(),
            data_root: self.cfg.path(),
            adapter_target: None,
            stop: &self.stop,
            launcher: &self.launcher,
            rng: crate::workspace::agent_name::mint::test_rng(),
        }
    }
}

/// The head of `config/default`.
pub(super) fn head(ws: &Path) -> String {
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
pub(super) const SWITCHED_WORKFLOW: &str =
    "events: {}\nretry:\n  max_attempts: 7\n  backoff: exponential\n";

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
    // With no mark the workflow's source IS the followed config commit,
    // so the step record's two shas agree (bl-e4a0) — the general path.
    assert_eq!(cfg.workflow_commit, cfg.config_commit);
}

#[test]
fn a_marked_agent_pins_the_marked_workflow_while_every_other_fact_follows_the_tip() {
    // The mark under follow-the-tip (§2.2 bl-403b × §6 bl-f928): control
    // follows the lineage's current head — the soul here — while the
    // marked commit keeps answering the workflow question alone. The
    // mark moves (and pins) policy, not identity.
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-r1");
    let fork = head(&ws);
    fixture::amend_config(
        &ws,
        &[
            ("workflow.yaml", SWITCHED_WORKFLOW),
            ("souls/worker.md", "a switched soul\n"),
        ],
    );
    let fx = Fx::new();
    workflow_mark::write(&ws, "20260101-r1", &fork, &fx.git).unwrap();
    let cfg = resolve_worker(&ws, ConfigSource::Agent("20260101-r1"), &fx.deps()).unwrap();
    assert_eq!(
        cfg.workflow.retry.max_attempts, 3,
        "the marked (fork) workflow stands against the moved tip",
    );
    assert!(
        cfg.soul.contains("a switched soul"),
        "every unmarked control fact follows the tip (operator ruling 2026-09-01)",
    );
    // The two shas the step record carries (bl-e4a0) disagree here, and
    // their disagreement IS the record that a mark stood: control came
    // from the tip, the workflow from the marked fork commit.
    assert_eq!(cfg.workflow_commit, fork);
    assert_eq!(cfg.config_commit, head(&ws));
    assert_ne!(cfg.workflow_commit, cfg.config_commit);
}

#[test]
fn an_unmarked_agent_follows_the_tips_workflow_like_every_other_control_fact() {
    // The inversion of the retired freeze pin: a config edit after the
    // fork reaches the running agent's workflow at its next step.
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-r1");
    fixture::amend_config(&ws, &[("workflow.yaml", SWITCHED_WORKFLOW)]);
    let fx = Fx::new();
    let cfg = resolve_worker(&ws, ConfigSource::Agent("20260101-r1"), &fx.deps()).unwrap();
    assert_eq!(cfg.workflow.retry.max_attempts, 7);
}

#[test]
fn clearing_the_mark_returns_resolution_to_the_followed_workflow() {
    // Cleared, the workflow rejoins every other control fact on the
    // lineage's current tip — not on the fork commit the retired freeze
    // would have answered.
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-r1");
    let fork = head(&ws);
    fixture::amend_config(&ws, &[("workflow.yaml", SWITCHED_WORKFLOW)]);
    let fx = Fx::new();
    workflow_mark::write(&ws, "20260101-r1", &fork, &fx.git).unwrap();
    workflow_mark::clear(&ws, "20260101-r1", &fx.git).unwrap();
    let cfg = resolve_worker(&ws, ConfigSource::Agent("20260101-r1"), &fx.deps()).unwrap();
    assert_eq!(cfg.workflow.retry.max_attempts, 7, "the tip's workflow");
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
fn diverged_lineages_resolve_the_fork_commit_and_say_so_loudly() {
    // The held arm (§2.2, `docs/DESIGN_CONFIG_FOLLOW.md`): a variant
    // lineage forked at the fork commit, then the default advanced —
    // two distinct tips reach the agent, so control resolves the fork
    // commit itself (the conservative pre-ruling answer, with the
    // per-step notice) rather than guessing a lineage.
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-r1");
    let fx = Fx::new();
    fx.git
        .run(
            &workspace::repo_git(&ws),
            &["update-ref", "refs/heads/config/variant", &head(&ws)],
        )
        .unwrap();
    fixture::amend_config(&ws, &[("workflow.yaml", SWITCHED_WORKFLOW)]);
    let cfg = resolve_worker(&ws, ConfigSource::Agent("20260101-r1"), &fx.deps()).unwrap();
    assert_eq!(
        cfg.workflow.retry.max_attempts, 3,
        "held on the fork commit: the advanced default's workflow does not reach it",
    );
}
