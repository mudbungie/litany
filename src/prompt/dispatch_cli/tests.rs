//! [`super::run_with`] tests: the guard sequence, the per-role `--goal`
//! rule, and the fork itself against real scaffolded workspaces.

use super::*;
use crate::workspace::fixture;
use tempfile::TempDir;

/// A real scaffolded workspace (the `litany new` core) with a parent
/// agent branch + worktree — the state `litany dispatch` is invoked
/// against in production (§3.4). The default config lists `worker` and
/// `compactor` with their souls, so both validate off this parent.
pub(super) fn scaffolded_repo_with_parent(parent: &str) -> (TempDir, std::path::PathBuf) {
    let (holder, repo) = fixture::workspace();
    fixture::spawn_root(&repo, parent);
    (holder, repo)
}

/// A [`Launcher`] that swallows launches — the fork + front-door
/// deposit is under test, not the real `litany advance` spawn.
pub(super) struct NoopLauncher;
impl Launcher for NoopLauncher {
    fn launch(&self, _workspace: &Path, _agent_id: &str) -> std::io::Result<()> {
        Ok(())
    }
}

/// Count sub-agent worktrees forked under `parent`'s id prefix.
pub(super) fn sub_count(repo: &Path, parent: &str) -> usize {
    std::fs::read_dir(repo.join(crate::workspace::AGENTS_DIR))
        .unwrap()
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with(&format!("{parent}-"))
        })
        .count()
}

#[test]
fn compactor_dispatch_forks_an_ordinary_compactor_child() {
    // §2.7: the compactor is an ordinary child dispatch — a branch
    // off the dispatching tip with the compactor soul pinned and a
    // boilerplate goal deposited, run by the front door.
    let (_holder, repo) = scaffolded_repo_with_parent("20260101-p1");
    run_with(
        ROLE_COMPACTOR,
        &repo,
        "20260101-p1",
        None,
        None,
        None,
        crate::prompt::PinnedDocs::none(),
        None,
        &NoopLauncher,
    )
    .unwrap();
    assert_eq!(sub_count(&repo, "20260101-p1"), 1);
}

#[test]
fn worker_dispatch_succeeds_and_spawns_a_sub_branch() {
    let (_holder, repo) = scaffolded_repo_with_parent("20260101-p1");
    run_with(
        "worker",
        &repo,
        "20260101-p1",
        Some("do the thing"),
        None,
        None,
        crate::prompt::PinnedDocs::none(),
        None,
        &NoopLauncher,
    )
    .unwrap();
    assert_eq!(sub_count(&repo, "20260101-p1"), 1);
}

#[test]
fn any_config_role_dispatches_open_set() {
    // The v0.7 criterion through the front door: a third role the
    // config defines (a verifier — zero code) is dispatchable exactly
    // like the template roles. No name list gates it.
    let (_holder, repo) = fixture::workspace();
    let yaml = "roles:\n  worker:\n    provider: anthropic\n    model: sonnet\n  \
                verifier:\n    provider: anthropic\n    model: sonnet\n";
    fixture::amend_config(
        &repo,
        &[("providers.yaml", yaml), ("souls/verifier.md", "v\n")],
    );
    fixture::spawn_root(&repo, "p9");
    run_with(
        "verifier",
        &repo,
        "p9",
        Some("judge it"),
        None,
        None,
        crate::prompt::PinnedDocs::none(),
        None,
        &NoopLauncher,
    )
    .unwrap();
    assert_eq!(sub_count(&repo, "p9"), 1);
}

// --- the shared id guard (bl-c89b) ----------------------------------
//
// README: "The id guard is the same rule at every verb taking an agent id
// from outside — `message`, `advance`, `stop`, `dispatch`, `bundle`."
// These three pin the claim for `dispatch`: the layout, the parent, and
// the role pool, each declined in the product's voice ahead of any
// governing-config derivation.

#[test]
fn a_path_that_is_not_a_workspace_is_the_shared_layout_decline() {
    let holder = TempDir::new().unwrap();
    let err = run_with(
        "worker",
        holder.path(),
        "someagent",
        Some("hi"),
        None,
        None,
        crate::prompt::PinnedDocs::none(),
        None,
        &NoopLauncher,
    )
    .unwrap_err();
    assert!(matches!(err, DispatchCliError::Layout(_)), "{err}");
    assert_eq!(
        err.to_string(),
        format!(
            "{} is not a workspace (no repo.git) — create one with `litany new` (ARCH §2.2)",
            holder.path().display()
        )
    );
}

#[test]
fn a_parent_with_no_agent_ref_is_the_shared_existence_decline() {
    let (_holder, repo) = scaffolded_repo_with_parent("p1");
    let err = run_with(
        "worker",
        &repo,
        "nosuchparent",
        Some("hi"),
        None,
        None,
        crate::prompt::PinnedDocs::none(),
        None,
        &NoopLauncher,
    )
    .unwrap_err();
    assert!(matches!(err, DispatchCliError::UnknownParent(_)), "{err}");
    assert_eq!(
        err.to_string(),
        "no agent \"nosuchparent\" in this workspace — a child forks off an existing parent \
         (ARCH §2.5); check the id against the workspace's `agents/*` refs, or start an \
         agent with `litany prompt` / `litany dispatch`"
    );
}

#[test]
fn undefined_role_names_the_roles_that_are_defined() {
    // No 40-hex sha, no `<sha>:providers.yaml` git-show form: the source
    // is named as the user knows it, and the pool that IS defined travels
    // with the refusal.
    let (_holder, repo) = scaffolded_repo_with_parent("p1");
    let err = run_with(
        "no-such-role",
        &repo,
        "p1",
        Some("g"),
        None,
        None,
        crate::prompt::PinnedDocs::none(),
        None,
        &NoopLauncher,
    )
    .unwrap_err();
    assert!(matches!(err, DispatchCliError::InvalidRole(_)), "{err}");
    assert_eq!(
        err.to_string(),
        "role \"no-such-role\" is not defined in the providers.yaml that will govern a \
         child of agent \"p1\" \
         — defined roles: compactor, worker"
    );
}

#[test]
fn worker_requires_a_goal() {
    // Through the public `run` (the AdvanceLauncher wiring): validation
    // passes, then the missing `--goal` is refused before any fork.
    let (_holder, repo) = scaffolded_repo_with_parent("p1");
    let err = run(
        "worker",
        &repo,
        "p1",
        None,
        None,
        None,
        crate::prompt::PinnedDocs::none(),
        None,
        Path::new("true"),
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "--goal is required for role \"worker\"");
}

#[test]
fn compactor_rejects_a_goal() {
    let (_holder, repo) = scaffolded_repo_with_parent("p1");
    let err = run(
        ROLE_COMPACTOR,
        &repo,
        "p1",
        Some("g"),
        None,
        None,
        crate::prompt::PinnedDocs::none(),
        None,
        Path::new("true"),
    )
    .unwrap_err();
    assert_eq!(
        err.to_string(),
        "--goal is not accepted for role \"compactor\""
    );
}

#[test]
fn inner_errors_render_through_the_shared_display() {
    // Validation passes, but the fork itself fails: with the parent's
    // worktree removed, `git worktree add` (run in it) errors, flowing
    // through `From<Error>` and the shared `Display`.
    let (_holder, repo) = scaffolded_repo_with_parent("p1");
    std::fs::remove_dir_all(repo.join(crate::workspace::AGENTS_DIR).join("p1")).unwrap();
    let err = run_with(
        "worker",
        &repo,
        "p1",
        Some("g"),
        None,
        None,
        crate::prompt::PinnedDocs::none(),
        None,
        &NoopLauncher,
    )
    .unwrap_err();
    assert!(matches!(err, DispatchCliError::Inner(_)), "{err}");
    assert!(!err.to_string().is_empty());
}

#[test]
fn dispatch_pins_are_committed_on_the_child_dispatch_commit() {
    // §2.5 caller-supplied pinned documents, the child-path twin of
    // `prompt::tests::pinned`: exact bytes land at the caller-named
    // destinations on the child's dispatch commit — inspectable from
    // the ref alone (`git show`), no sidecar copy — and ordinary fork
    // inheritance carries them from there.
    let (_holder, repo) = scaffolded_repo_with_parent("20260101-p1");
    let pins = crate::prompt::PinnedDocs::new(vec![
        crate::prompt::PinnedDoc::new("AGENTS.md".into(), b"project law".to_vec()).unwrap(),
        crate::prompt::PinnedDoc::new("docs/notes.md".into(), b"nested".to_vec()).unwrap(),
    ])
    .unwrap();
    run_with(
        "worker",
        &repo,
        "20260101-p1",
        Some("do the thing"),
        None,
        None,
        &pins,
        None,
        &NoopLauncher,
    )
    .unwrap();

    let agents = repo.join(crate::workspace::AGENTS_DIR);
    let child = std::fs::read_dir(&agents)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .find(|n| n.starts_with("20260101-p1-"))
        .unwrap();
    // Committed, not merely written: read through the ref.
    use crate::template::GitRunner;
    let git = crate::template::RealGit::new();
    let repo_git = crate::workspace::repo_git(&repo);
    let show = |path: &str| {
        git.run_capture(&repo_git, &["show", &format!("agents/{child}:{path}")])
            .unwrap()
    };
    assert_eq!(show("AGENTS.md"), "project law");
    assert_eq!(show("docs/notes.md"), "nested");
    // Beside the ordinary dispatch artifacts, not instead of them.
    assert_eq!(show("goal.md"), "do the thing");
}
