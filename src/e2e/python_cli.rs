//! A **program** through the process boundary
//! (`docs/DESIGN_CODE_EXECUTION.md` §2.2, §2.4): `litany tool python`
//! with a program that composes two inner invocations of its own.
//!
//! What this pins that an in-process test cannot. The stub module's
//! functions are real `litany invoke` executions — a program reaches its
//! tools through the front door and nothing else — each landing its own
//! record under the derived id `<tool-id>-<k>` in program order. And the
//! part the operator's ruling turns on: **only the program's stdout
//! comes back**. No inner result appears in it, not a line and not a
//! tally, and the agent's branch gains no commit and no transcript
//! entry from either invocation.

use crate::harness_root::Roots;
use crate::prompt::step::STEPS_DIR;
use crate::prompt::tool::{ENV_TOOL_ID, STEP_TOOLS_SUBDIR};
use crate::template::{GitRunner, RealGit};
use crate::test_support::litany_binary;
use crate::workspace::fixture;
use std::io::Write;
use std::process::{Command, Stdio};

const AGENT: &str = "20260101-a1";
const TOOL_ID: &str = "tu_program";

/// The program: two inner invocations, joined, with one line printed.
/// Its own reading of what happened is what the model will read — the
/// failing one included, which it reads off a `Result` rather than
/// catching (a program filtering failures wants the code, §2.7). Each
/// `.stdout` is that invocation's raw result envelope, so its last line
/// is the tool's own last line of output.
const PROGRAM: &str = r#"
import litany_tools

first = litany_tools.bash(command="printf alpha")
second = litany_tools.bash(command="printf beta; exit 3")
print("ran 2, ok=%d, codes=%s, joined=%s" % (
    sum(1 for r in (first, second) if r.ok),
    [first.exit_code, second.exit_code],
    first.stdout.splitlines()[-1] + second.stdout.splitlines()[-1],
))
"#;

#[test]
fn a_program_composes_two_inner_invocations_and_only_its_stdout_comes_back() {
    let holder = tempfile::TempDir::new().unwrap();
    let home = holder.path().join("home");
    let roots = Roots {
        config: home.clone(),
        data: home.clone(),
    };
    let ws = fixture::workspace_under(&roots);
    // Name an adapter binary so the §4.4 load-time version guard is
    // skipped: a program makes no model call, so the target is never
    // spawned and the verdict never depends on which `bz` this box has.
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

    let litany = litany_binary();
    let input = serde_json::json!({ "program": PROGRAM }).to_string();
    let mut child = Command::new(&litany)
        .arg("tool")
        .arg("python")
        .current_dir(&worktree)
        .env("LITANY_HOME", &home)
        .env(crate::prompt::tool::ENV_CONV_REPO, &ws)
        .env(crate::prompt::tool::ENV_CONV_BRANCH, AGENT)
        .env(ENV_TOOL_ID, TOOL_ID)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn litany tool python");
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(input.as_bytes())
        .expect("write the program");
    let out = child.wait_with_output().expect("reap litany tool python");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(out.status.success(), "{stdout}{stderr}");

    // The program's own line, and nothing else: the second invocation
    // exited 3 and the program read it as a value rather than dying.
    assert_eq!(
        stdout, "ran 2, ok=1, codes=[0, 3], joined=alphabeta\n",
        "{stderr}"
    );
    assert!(
        !stdout.contains("Exit code:"),
        "an inner result envelope reached the model: {stdout}"
    );

    // Both inner invocations recorded under the in-flight step, in
    // program order, beside the module the program imported them from.
    let tools = step.join(STEP_TOOLS_SUBDIR);
    assert!(tools.join(format!("{TOOL_ID}-1")).is_dir(), "{stderr}");
    assert!(tools.join(format!("{TOOL_ID}-2")).is_dir(), "{stderr}");
    assert!(tools.join(TOOL_ID).join("litany_tools.py").is_file());

    // And the branch is untouched: the inner invocations commit nothing
    // and enter no transcript.
    let after = git.run_capture(&worktree, &["rev-parse", "HEAD"]).unwrap();
    assert_eq!(before, after, "an inner invocation commits nothing");
    assert!(
        !worktree
            .join(crate::prompt::dispatch::MESSAGES_DIR)
            .exists(),
        "no transcript entry is written for an inner invocation"
    );
}
