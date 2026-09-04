//! Context files on the tool result (ARCH §3.3 *Context files ride the
//! next tool result*, `docs/DESIGN_CONTEXT_ECONOMY.md` §6): what the
//! path of the agent's working directory carries, what the transcript
//! says it has already seen, and where the append lands.

use super::{NoAdapter, NoLauncher, NoSleeper, Recorder, Resolution, branch_with_step};
use crate::config::Workflow;
use crate::template::{GitRunner, RealGit};
use brazen::Content;
use serde_json::json;
use std::io;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use tempfile::TempDir;

/// A workflow declaring the one name these beats plant.
const AGENTS_ONLY: &str = "events: {}\ncontext_files: [AGENTS.md]\n";

/// A repository carrying `AGENTS.md` at its top level and a `sub/`
/// directory carrying its own, plus a `CLAUDE.md` no list below names.
fn repo_with_context_files(git: &RealGit) -> TempDir {
    let dir = TempDir::new().unwrap();
    git.run(dir.path(), &["init"]).unwrap();
    std::fs::write(dir.path().join("AGENTS.md"), "top rules\n").unwrap();
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    std::fs::write(dir.path().join("sub/AGENTS.md"), "sub rules\n").unwrap();
    std::fs::write(dir.path().join("sub/CLAUDE.md"), "claude rules\n").unwrap();
    dir
}

/// Seat the agent's working-directory mark at `dir` — the real §3.3
/// mark, through the real writer, so these beats read what a `cd` or a
/// `--cwd` seed would leave.
fn seated_at(ws: &TempDir, agent_id: &str, dir: &Path, git: &RealGit) {
    let repo = crate::workspace::repo_git(ws.path());
    git.run(ws.path(), &["init", "--bare", repo.to_str().unwrap()])
        .unwrap();
    let resolved = crate::workspace::cwd::resolve(dir).unwrap();
    crate::workspace::cwd::write(ws.path(), agent_id, &resolved, git).unwrap();
}

/// One tool result's bytes after the append, starting from an ordinary
/// envelope.
fn appended(ws: &TempDir, worktree: &Path, agent: &str, yaml: &str, git: &dyn GitRunner) -> String {
    let workflow = Workflow::parse(yaml, Path::new("workflow.yaml")).unwrap();
    let mut content = b"Exit code: 0\nok\n".to_vec();
    super::super::context::append(&mut content, ws.path(), worktree, agent, &workflow, git)
        .unwrap();
    String::from_utf8(content).unwrap()
}

/// A committed tool entry framing `path` — what the shown query reads
/// (§2.3: the entry is the record).
fn entry_framing(worktree: &Path, seq: &str, path: &Path) {
    let block = Content::ToolResult {
        tool_use_id: "t0".into(),
        content: vec![Content::Text(format!(
            "Exit code: 0\n<file path=\"{}\">\nrules\n</file>\n",
            path.display()
        ))],
        is_error: false,
    };
    let entry = serde_json::to_vec(&[block]).unwrap();
    std::fs::write(worktree.join(format!("messages/{seq}-tool.json")), entry).unwrap();
}

#[test]
fn every_directory_from_the_repository_top_down_to_the_cwd_rides_top_first() {
    let agent_id = "agent-b66b";
    let ws = TempDir::new().unwrap();
    let git = RealGit::new();
    let (worktree, _) = branch_with_step(&ws, agent_id, &git);
    let repo = repo_with_context_files(&git);
    seated_at(&ws, agent_id, &repo.path().join("sub"), &git);

    let out = appended(&ws, &worktree, agent_id, AGENTS_ONLY, &git);
    let top = out.find("top rules").expect("the top-level file rides");
    let sub = out.find("sub rules").expect("the cwd's own file rides");
    assert!(top < sub, "the path is walked downward: top first\n{out}");
    // A name the list does not carry is never read, though it sits in
    // the same directory as one that is.
    assert!(!out.contains("claude rules"), "{out}");
}

#[test]
fn an_omitted_list_discovers_nothing() {
    // The general path with the policy absent: no stat, no read, and
    // the result reaches the transcript exactly as the envelope wrote
    // it.
    let agent_id = "agent-none";
    let ws = TempDir::new().unwrap();
    let git = RealGit::new();
    let (worktree, _) = branch_with_step(&ws, agent_id, &git);
    let repo = repo_with_context_files(&git);
    seated_at(&ws, agent_id, repo.path(), &git);

    let out = appended(&ws, &worktree, agent_id, "events: {}\n", &git);
    assert_eq!(out, "Exit code: 0\nok\n");
}

#[test]
fn a_file_a_committed_entry_already_frames_is_not_shown_again() {
    let agent_id = "agent-shown";
    let ws = TempDir::new().unwrap();
    let git = RealGit::new();
    let (worktree, _) = branch_with_step(&ws, agent_id, &git);
    let repo = repo_with_context_files(&git);
    seated_at(&ws, agent_id, repo.path(), &git);
    let framed = std::fs::canonicalize(repo.path().join("AGENTS.md")).unwrap();
    entry_framing(&worktree, "002", &framed);

    assert_eq!(
        appended(&ws, &worktree, agent_id, AGENTS_ONLY, &git),
        "Exit code: 0\nok\n"
    );
    // A compaction that removes the carrying entry shows it again —
    // right, because the model lost it (§6). The state is the
    // transcript's, so no mark has to be unwound.
    std::fs::remove_file(worktree.join("messages/002-tool.json")).unwrap();
    assert!(appended(&ws, &worktree, agent_id, AGENTS_ONLY, &git).contains("top rules"));
}

#[test]
fn a_cwd_outside_any_repository_carries_that_directory_alone() {
    let agent_id = "agent-loose";
    let ws = TempDir::new().unwrap();
    let git = RealGit::new();
    let (worktree, _) = branch_with_step(&ws, agent_id, &git);
    let loose = TempDir::new().unwrap();
    std::fs::write(loose.path().join("AGENTS.md"), "loose rules").unwrap();
    seated_at(&ws, agent_id, loose.path(), &git);

    // No enclosing tree is the general path with it absent — one
    // directory, and the unterminated file still gains its separator.
    let out = appended(&ws, &worktree, agent_id, AGENTS_ONLY, &git);
    assert!(out.ends_with("\nloose rules\n</file>\n"), "{out}");
}

/// A [`GitRunner`] answering every read with a path that is nobody's
/// parent — the mark read included, so the cwd falls back to the
/// worktree and the toplevel then fails to be its prefix.
struct Astray;
impl GitRunner for Astray {
    fn run(&self, _dest: &Path, _args: &[&str]) -> io::Result<()> {
        unreachable!("the append only captures")
    }
    fn run_capture(&self, _dest: &Path, _args: &[&str]) -> io::Result<String> {
        Ok("/no/such/toplevel".to_string())
    }
}

#[test]
fn a_toplevel_that_is_not_the_cwds_prefix_falls_back_to_the_cwd_alone() {
    let agent_id = "agent-astray";
    let ws = TempDir::new().unwrap();
    let git = RealGit::new();
    let (worktree, _) = branch_with_step(&ws, agent_id, &git);
    std::fs::write(worktree.join("AGENTS.md"), "worktree rules").unwrap();

    let out = appended(&ws, &worktree, agent_id, AGENTS_ONLY, &Astray);
    assert!(out.contains("worktree rules"), "{out}");
}

#[test]
fn a_result_not_ending_in_a_newline_gains_a_separator_before_the_frame() {
    let agent_id = "agent-sep";
    let ws = TempDir::new().unwrap();
    let git = RealGit::new();
    let (worktree, _) = branch_with_step(&ws, agent_id, &git);
    std::fs::write(worktree.join("AGENTS.md"), "worktree rules\n").unwrap();
    let workflow = Workflow::parse(AGENTS_ONLY, Path::new("workflow.yaml")).unwrap();

    let mut content = b"Exit code: 0\nno newline".to_vec();
    super::super::context::append(
        &mut content,
        ws.path(),
        &worktree,
        agent_id,
        &workflow,
        &git,
    )
    .unwrap();
    let out = String::from_utf8(content).unwrap();
    assert!(
        out.starts_with("Exit code: 0\nno newline\n<file path="),
        "{out}"
    );
}

#[test]
fn the_seeded_cwds_files_ride_the_first_result_and_no_later_one() {
    // `--cwd` seeds the mark at creation and has no tool result of its
    // own to append to (§3.3), so the carrier is whichever result comes
    // first — and the transcript it commits into is what stops the
    // second from repeating it.
    let agent_id = "agent-seed";
    let ws = TempDir::new().unwrap();
    let git = RealGit::new();
    let (worktree, step_dir_rel) = branch_with_step(&ws, agent_id, &git);
    let repo = repo_with_context_files(&git);
    seated_at(&ws, agent_id, repo.path(), &git);

    let mut resolution = Resolution::new();
    resolution.workflow = Workflow::parse(AGENTS_ONLY, Path::new("workflow.yaml")).unwrap();
    let recorder = Recorder(std::cell::RefCell::new(Vec::new()));
    let stop = AtomicBool::new(false);
    let clock = crate::prompt::clock::SystemClock;
    let id_gen = crate::prompt::NanoIdGen;
    let cfg = TempDir::new().unwrap();
    let deps = crate::prompt::Deps {
        adapter: &NoAdapter,
        sleeper: &NoSleeper,
        git: &git,
        clock: &clock,
        id_gen: &id_gen,
        tool_executor: &recorder,
        config_root: cfg.path(),
        data_root: cfg.path(),
        adapter_target: None,
        stop: &stop,
        launcher: &NoLauncher,
        rng: crate::workspace::agent_name::mint::test_rng(),
    };
    let grant = ["bash".to_string()];
    for id in ["t1", "t2"] {
        let content = vec![Content::ToolUse {
            id: id.into(),
            name: "bash".into(),
            input: json!({"command": "true"}),
            signature: None,
        }];
        super::super::run_tool_calls(
            ws.path(),
            &worktree,
            agent_id,
            &resolution.of(crate::prompt::WORKER_ROLE, &grant),
            &step_dir_rel,
            &content,
            &deps,
        )
        .unwrap();
    }
    let first = std::fs::read_to_string(worktree.join("messages/002-tool.json")).unwrap();
    let second = std::fs::read_to_string(worktree.join("messages/003-tool.json")).unwrap();
    assert!(first.contains("top rules"), "{first}");
    assert!(!second.contains("top rules"), "{second}");
}
