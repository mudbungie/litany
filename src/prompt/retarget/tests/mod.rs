//! The retarget landing (ARCH §2.2) against a **real** workspace, so the
//! re-derived dispatch commit, the replay, and the ancestry query that
//! answers afterwards are exercised end to end. The git-op error arms live
//! in [`stub`].

use super::*;
use crate::template::RealGit;
use crate::workspace::{
    DEFAULT_CONFIG_NAME, agent_ref, agent_worktree, config_ref, fixture, repo_git,
};
use std::path::PathBuf;
use tempfile::TempDir;

mod preflighting;
mod stub;

fn g() -> RealGit {
    RealGit::new()
}

/// The head commit of `config/<name>`.
fn head_of(ws: &Path, name: &str) -> String {
    g().run_capture(&repo_git(ws), &["rev-parse", &config_ref(name)])
        .unwrap()
        .trim()
        .to_string()
}

/// A **root** agent, forked off `config/default` and founded by the real
/// root dispatch commit (`step 001: dispatch [<id>]`, §2.3 step 2) — the
/// shape a retarget addresses. The trim is the production one, so the
/// branch starts with exactly the tree a fork leaves.
fn root(ws: &Path, id: &str) -> PathBuf {
    let git = g();
    let wt = agent_worktree(ws, id);
    let wt_str = wt.to_string_lossy().to_string();
    git.run(
        &repo_git(ws),
        &[
            "worktree",
            "add",
            "-b",
            &agent_ref(id),
            &wt_str,
            &config_ref(DEFAULT_CONFIG_NAME),
        ],
    )
    .unwrap();
    let commit = head_of(ws, DEFAULT_CONFIG_NAME);
    let tools = base::granted(ws, &commit, WORKER_ROLE, &git).unwrap();
    let grant = dispatch::Grant {
        role: WORKER_ROLE,
        tools: &tools,
        config_commit: &commit,
    };
    dispatch::trim_to_context(&wt, "20260101-t1", &grant, Some("pale-otter"), &git).unwrap();
    std::fs::write(wt.join("goal.md"), "ship the widget\n").unwrap();
    let soul = crate::workspace::show_control(ws, &commit, "souls/worker.md", &git).unwrap();
    std::fs::write(wt.join("soul.md"), soul).unwrap();
    git.run(&wt, &["add", "goal.md", "soul.md"]).unwrap();
    git.run(
        &wt,
        &["commit", "-m", &format!("step 001: dispatch [{id}]")],
    )
    .unwrap();
    wt
}

/// A workspace with one root agent `a`, ready to retarget. Shared with
/// the boundary test in `prompt::tests`, which proves the executor is
/// what consumes the mark (§2.2).
pub(crate) fn agent() -> (TempDir, PathBuf, PathBuf) {
    let (holder, ws) = fixture::workspace();
    let wt = root(&ws, "a");
    (holder, ws, wt)
}

/// One ordinary branch commit — a transcript entry, as the executor lands
/// them (§2.3): a new file under a monotonic name.
fn step(wt: &Path, rel: &str, content: &str, subject: &str) {
    let git = g();
    let path = wt.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
    git.run(wt, &["add", "-A"]).unwrap();
    git.run(wt, &["commit", "-m", subject]).unwrap();
}

fn subjects(wt: &Path) -> Vec<String> {
    g().run_capture(wt, &["log", "--format=%s"])
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect()
}

/// A diverged `config/variant` lineage: forked at `config/default`'s
/// current head, then advanced with `files`. Under follow-the-tip
/// (§2.2, bl-403b) a same-lineage advance reaches the agent by
/// resolution alone, so what a retarget addresses is a change of
/// **lineage** — this shape.
pub(crate) fn variant(ws: &Path, files: &[(&str, &str)]) -> String {
    g().run(
        &repo_git(ws),
        &[
            "update-ref",
            "refs/heads/config/variant",
            &head_of(ws, DEFAULT_CONFIG_NAME),
        ],
    )
    .unwrap();
    fixture::amend_lineage(ws, "variant", files);
    head_of(ws, "variant")
}

/// Mark and land in one move, as an executor's boundary does.
fn retarget(ws: &Path, wt: &Path, target: &str) -> Option<Outcome> {
    crate::workspace::retarget::write(ws, "a", target, &g()).unwrap();
    land(ws, "a", wt, &g()).unwrap()
}

#[test]
fn a_retarget_re_forks_the_branch_onto_the_target_and_replays_its_history() {
    // THE PIN (§2.2): the branch's governing config commit is a pure
    // ancestry query and answers the *target* afterwards — with the
    // agent's own history intact on top of the re-derived dispatch commit.
    let (_h, ws, wt) = agent();
    let before = head_of(&ws, DEFAULT_CONFIG_NAME);
    step(
        &wt,
        "messages/001-user.md",
        "hi\n",
        "transcript 001: user [a]",
    );
    step(&wt, "code.txt", "v1\n", "transcript 002: tool [a]");
    let target = variant(&ws, &[("souls/worker.md", "a newer soul\n")]);
    assert_ne!(before, target);

    assert_eq!(retarget(&ws, &wt, &target), Some(Outcome::Landed));

    assert_eq!(governing(&ws, &agent_ref("a"), &g()).unwrap(), target);
    // The tail survived verbatim, and the branch is attached and clean.
    assert_eq!(
        subjects(&wt)[..3],
        [
            "transcript 002: tool [a]",
            "transcript 001: user [a]",
            "step 001: dispatch [a]",
        ],
    );
    assert_eq!(
        std::fs::read_to_string(wt.join("code.txt")).unwrap(),
        "v1\n"
    );
    assert_eq!(
        std::fs::read_to_string(wt.join("goal.md")).unwrap(),
        "ship the widget\n",
        "the goal is the agent's own and is never re-derived (§2.8)",
    );
    assert_eq!(
        g().run_capture(&wt, &["symbolic-ref", "--short", "HEAD"])
            .unwrap(),
        agent_ref("a"),
    );
    assert_eq!(
        g().run_capture(&wt, &["status", "--porcelain"]).unwrap(),
        ""
    );
    assert!(
        g().run_capture(&wt, &["rev-list", "--merges", "HEAD"])
            .unwrap()
            .trim()
            .is_empty(),
        "nothing merges anywhere (§2.3, §2.6)",
    );
}

#[test]
fn the_soul_is_re_pinned_from_the_target_config() {
    // §2.3 pins the soul at the dispatch commit; a retarget re-mints that
    // commit, so the pin is re-read from the target — which is the point
    // of retargeting a lineage whose soul moved.
    let (_h, ws, wt) = agent();
    let target = variant(&ws, &[("souls/worker.md", "a newer soul\n")]);
    retarget(&ws, &wt, &target);
    assert_eq!(
        std::fs::read_to_string(wt.join("soul.md")).unwrap(),
        "a newer soul",
        "control reads are trimmed at the git boundary, as every fork's is",
    );
}

#[test]
fn the_descriptor_cut_is_re_derived_from_the_target_not_replayed() {
    // §3.3: the descriptors are a *view* of the target config's snapshot,
    // cut to the role's grant there — never the tree the branch carried.
    // A target that narrows the grant narrows the branch's own tree.
    let (_h, ws, wt) = agent();
    assert!(wt.join("descriptions/tools/bash.json").exists());
    let target = variant(
        &ws,
        &[(
            "providers.yaml",
            "roles:\n  worker:\n    provider: anthropic\n    model: claude-sonnet-5\n    \
             tools: [read_file]\n",
        )],
    );
    retarget(&ws, &wt, &target);
    assert!(wt.join("descriptions/tools/read_file.json").exists());
    assert!(
        !wt.join("descriptions/tools/bash.json").exists(),
        "an ungranted tool's descriptor leaves with the cut",
    );
}

#[test]
fn the_control_files_stay_gone_so_no_modify_delete_can_arise() {
    // The base is minted *with* the removal re-performed (§2.2), which is
    // why the naive rebase's modify/delete on providers.yaml — the new
    // parent carries it, the old dispatch commit deletes it — has no
    // occasion to happen.
    let (_h, ws, wt) = agent();
    let target = variant(&ws, &[("souls/worker.md", "an amended soul\n")]);
    assert_eq!(retarget(&ws, &wt, &target), Some(Outcome::Landed));
    for control in crate::workspace::CONTROL_PATHS {
        assert!(
            !wt.join(control).exists(),
            "{control} is control, not context"
        );
    }
}

#[test]
fn a_target_already_governing_the_agent_is_a_clean_no_op() {
    let (_h, ws, wt) = agent();
    let current = head_of(&ws, DEFAULT_CONFIG_NAME);
    let tip = g().run_capture(&wt, &["rev-parse", "HEAD"]).unwrap();
    assert_eq!(retarget(&ws, &wt, &current), Some(Outcome::NoOp));
    assert_eq!(g().run_capture(&wt, &["rev-parse", "HEAD"]).unwrap(), tip);
}

#[test]
fn an_unmarked_branch_lands_nothing_which_is_every_boundary_but_one() {
    let (_h, ws, wt) = agent();
    assert_eq!(land(&ws, "a", &wt, &g()).unwrap(), None);
}

#[test]
fn the_mark_is_consumed_whatever_the_outcome() {
    let (_h, ws, wt) = agent();
    let target = variant(&ws, &[("souls/worker.md", "an amended soul\n")]);
    retarget(&ws, &wt, &target);
    assert_eq!(crate::workspace::retarget::read(&ws, "a", &g()), None);
    // And a no-op consumes it too — the question was answered.
    retarget(&ws, &wt, &head_of(&ws, "variant"));
    assert_eq!(crate::workspace::retarget::read(&ws, "a", &g()), None);
}

#[test]
fn a_failed_landing_still_consumes_the_mark_rather_than_re_asking() {
    // A branch with no dispatch commit is not a branch a retarget can
    // re-fork; the error surfaces, and the mark does not survive to fail
    // again at every subsequent boundary.
    let (_h, ws) = fixture::workspace();
    let wt = fixture::spawn_root(&ws, "a"); // subject `dispatch`, not a founding one
    let target = variant(&ws, &[("souls/worker.md", "an amended soul\n")]);
    crate::workspace::retarget::write(&ws, "a", &target, &g()).unwrap();
    let err = land(&ws, "a", &wt, &g()).unwrap_err();
    assert!(
        err.to_string().contains("no dispatch commit founds"),
        "{err}"
    );
    assert_eq!(crate::workspace::retarget::read(&ws, "a", &g()), None);
}
