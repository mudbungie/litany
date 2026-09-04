//! `litany invoke` through the process boundary, reached the way a
//! composing tool reaches it: from inside a `bash` tool invocation
//! (`docs/DESIGN_CODE_EXECUTION.md` §2.1).
//!
//! What this pins that an in-process test cannot: the verb is a real
//! front door — an agent's own shell can raise an inner invocation with
//! nothing but the §3.3 contract environment it already has — and the
//! invocation leaves the branch alone. Its record lands under the
//! in-flight step, its envelope comes back on the outer tool's stdout,
//! and the agent's transcript gains nothing: no commit, no
//! `messages/` entry. Only the composing tool's own output is what the
//! model will read.

use crate::harness_root::Roots;
use crate::prompt::step::STEPS_DIR;
use crate::prompt::tool::STEP_TOOLS_SUBDIR;
use crate::template::{GitRunner, RealGit};
use crate::test_support::litany_binary;
use crate::workspace::{agent_worktree, fixture};
use std::io::Write;
use std::process::{Command, Stdio};

const AGENT: &str = "20260101-a1";

#[test]
fn a_bash_tool_call_raises_an_inner_invocation_that_the_transcript_never_sees() {
    let holder = tempfile::TempDir::new().unwrap();
    let home = holder.path().join("home");
    let roots = Roots {
        config: home.clone(),
        data: home.clone(),
    };
    let ws = fixture::workspace_under(&roots);
    // Name an adapter binary so the §4.4 load-time version guard is
    // skipped: a door invocation makes no model call, so the target is
    // never spawned and the verdict never depends on which `bz` this
    // box has installed.
    std::fs::write(
        home.join("models.yaml"),
        format!("adapter: {}\n", home.join("no-adapter").display()),
    )
    .unwrap();
    let worktree = fixture::spawn_root(&ws, AGENT);
    let step = ws.join(STEPS_DIR).join(AGENT).join("001");
    std::fs::create_dir_all(&step).unwrap();
    let git = RealGit::new();
    let before = git.run_capture(&worktree, &["rev-parse", "HEAD"]).unwrap();

    // The outer tool call: `bash`, whose command pipes one `tool_use`
    // block into the door verb — an agent composing a tool call.
    let litany = litany_binary();
    let inner = r#"{"id":"tu_outer-1","name":"bash","input":{"command":"echo inner"}}"#;
    let command = format!("printf '%s' '{inner}' | {} invoke", litany.display());
    let outer = serde_json::json!({ "command": command }).to_string();
    let mut child = Command::new(&litany)
        .arg("tool")
        .arg("bash")
        .current_dir(&worktree)
        .env("LITANY_HOME", &home)
        .env(crate::prompt::tool::ENV_CONV_REPO, &ws)
        .env(crate::prompt::tool::ENV_CONV_BRANCH, AGENT)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn litany tool bash");
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(outer.as_bytes())
        .expect("write the outer tool input");
    let out = child.wait_with_output().expect("reap litany tool bash");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(out.status.success(), "{stdout}{stderr}");
    // The inner envelope rode back out through the outer tool's stdout,
    // raw — the exit-code line included (§3.3 *Result envelope*).
    assert!(stdout.contains("Exit code: 0\ninner\n"), "{stdout}{stderr}");

    // The inner invocation recorded under the in-flight step, beside
    // where the outer call's own record would land.
    assert!(
        step.join(STEP_TOOLS_SUBDIR).join("tu_outer-1").is_dir(),
        "the inner record lands under the in-flight step"
    );
    // And the branch is untouched: no commit, no transcript entry.
    let after = git.run_capture(&worktree, &["rev-parse", "HEAD"]).unwrap();
    assert_eq!(before, after, "the door commits nothing");
    assert!(
        !worktree
            .join(crate::prompt::dispatch::MESSAGES_DIR)
            .exists(),
        "no transcript entry is written for an inner invocation"
    );
    assert_eq!(agent_worktree(&ws, AGENT), worktree);
}
