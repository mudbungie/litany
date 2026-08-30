//! Integration test: the opt-in agent→agent cascade, over two *live*
//! executors (ARCH §2.9, README "Stopping a conversation").
//!
//! A parent root and one dispatched child, each blocked in its own model
//! call against a stalling mock, each holding its own inbox-directory
//! lock fd (§2.11) and its own process group (§2.9 — `litany prompt`
//! `setpgid`s, a launched `litany advance` is `setsid`-detached). Two
//! executors, two groups, one prefix in the id namespace.
//!
//! - `litany stop --stop-children` walks that namespace — the
//!   descendants of `<agent>` are exactly the inbox directories prefixed
//!   `<agent>-`, one scan reaching every depth — and folds the child's
//!   group into the same SIGTERM sweep. Both executors take the §2.9
//!   step-3 terminal sequence landed by bl-5156: the group signal fells
//!   `bz` mid-model-call, the pending stop flag (not the shape of the
//!   error `bz`'s death left behind) classifies, and the executor
//!   deposits its `stopped` result and exits **cleanly**. The child's
//!   `response.json` is left closed without a terminal `end` — the §2.9
//!   on-disk stop signature — and, being a child, its deposit is
//!   observable in the parent's inbox rather than the root no-op the
//!   sibling `stop_cli.rs` tests see.
//! - A bare `litany stop` (no flag) does **not** fell the child: the
//!   agent-boundary promise. Parent and child are separate agents, not a
//!   process hierarchy, so the kernel group signal cannot leak across
//!   and no CLI-level walk is performed.

use super::poll;
use super::stop_common::{
    HAPPY_SSE, litany_bin, poll_for_conv_branch_with_diag, poll_for_path, reap, scaffold_repo,
    spawn_prompt, write_brazen_config, write_global_models,
};
use crate::prompt::inbox::inbox_dir;
use crate::prompt::stop::{PgidFinder, ProcFsFinder};
use httpmock::Method::POST;
use httpmock::MockServer;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tempfile::TempDir;

/// A live parent/child pair, both mid-model-call. Field order is drop
/// order: the reaped `prompt` handle and the process-owning temp tree go
/// last, after the tests have felled everything they started.
struct Family {
    _server: MockServer,
    _tmp: TempDir,
    dest: PathBuf,
    parent: String,
    child: String,
    prompt: Child,
    child_pgid: i32,
}

/// `steps/<agent>/001/response.json` — the step-1 model-call record
/// (§2.3). Its existence proves the executor reached its model call; the
/// stalling mock holds it open there.
fn step_response(dest: &Path, agent: &str) -> PathBuf {
    dest.join("steps")
        .join(agent)
        .join("001")
        .join("response.json")
}

/// The pgid of whoever holds `agent`'s inbox-directory lock fd — the
/// product's own §2.9 discovery, reused as the test's aliveness probe.
/// `None` once the executor's fds are gone (it exited), which is a
/// sharper signal than `kill(pid, 0)`: a reparented orphan can linger as
/// a zombie, and a zombie holds no fds.
fn holder_pgid(dest: &Path, agent: &str) -> Option<i32> {
    ProcFsFinder::default()
        .find_holder_pgid(&inbox_dir(dest, agent))
        .expect("scan /proc")
}

/// Block until nobody holds `agent`'s inbox lock. A dying executor writes
/// as it goes (its truncated step record, its deposit, its commits), so
/// [`poll`]'s silence bound covers this wait as it covers the others: the
/// verdict is "the workspace stopped moving and the holder is still
/// there", never "it took too long".
fn poll_until_no_holder(dest: &Path, agent: &str) {
    let gone = poll::until(dest, || holder_pgid(dest, agent).is_none().then_some(()));
    assert!(
        gone.is_some(),
        "executor for {agent} (pgid {:?}) outlived the stop, with {} untouched for {:?}",
        holder_pgid(dest, agent),
        dest.display(),
        poll::patience()
    );
}

/// `litany stop <dest> <agent> [--stop-children]` through the CLI.
fn run_stop(dest: &Path, agent: &str, stop_children: bool) {
    let mut cmd = Command::new(litany_bin());
    cmd.arg("stop").arg(dest).arg(agent);
    if stop_children {
        cmd.arg("--stop-children");
    }
    let out = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("spawn litany stop");
    assert!(
        out.status.success(),
        "litany stop: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// §2.9 on-disk stop signature: the file is closed and its last JSONL
/// line — if it has one at all — is not the terminal `end`.
fn assert_no_terminal_end(path: &Path) {
    let bytes = fs::read(path).expect("stopped response.json is readable");
    let Some(last) = bytes.split(|b| *b == b'\n').rfind(|l| !l.is_empty()) else {
        return; // an empty file: cut down before the first event line.
    };
    let v: serde_json::Value = serde_json::from_slice(last).expect("trailing line is JSON");
    assert_ne!(
        v["type"].as_str(),
        Some("end"),
        "a stopped step's response.json carries no terminal `end`; last: {v}"
    );
}

/// A parent root and one dispatched child, both blocked in a model call
/// against a mock that never answers within the test's life.
///
/// The child is started through the front door — `litany dispatch worker`
/// (§3.4), the same fork-plus-deposit the model's `dispatch` tool takes
/// (§2.5) — whose deposit launches the child's ordinary driver, `litany
/// advance`, detached into its own process group. Driving a real
/// `dispatch` tool call through the model would exercise the identical
/// primitive one indirection further out.
fn live_family() -> Family {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(POST).path("/v1/messages");
        // Both executors block here, holding their inbox lock fds, until
        // the stop cuts the cord.
        then.status(200)
            .header("content-type", "text/event-stream")
            .delay(Duration::from_secs(120))
            .body(HAPPY_SSE);
    });

    let tmp = TempDir::new().unwrap();
    let harness = tmp.path().join("harness");
    fs::create_dir_all(&harness).unwrap();
    write_global_models(&harness);
    let brazen_config = write_brazen_config(tmp.path(), &server.base_url());
    let dest = tmp.path().join("conv");
    scaffold_repo(&dest, &harness);

    let mut prompt = spawn_prompt(&dest, &harness, &brazen_config, "ping");
    let parent = poll_for_conv_branch_with_diag(&dest, &mut prompt);
    poll_for_path(&dest, &step_response(&dest, &parent));

    // Fork the child off the live parent. `litany dispatch` is
    // writer-shaped (§2.1) — it forks, deposits, launches, and exits — so
    // its own exit leaves the child executor as the only live process on
    // the child branch.
    let out = Command::new(litany_bin())
        .args(["dispatch", "worker"])
        .arg(&dest)
        .arg(&parent)
        .arg("--goal")
        .arg("hold a model call open")
        .env("LITANY_HOME", &harness)
        .env("BRAZEN_CONFIG", &brazen_config)
        .output()
        .expect("spawn litany dispatch");
    assert!(
        out.status.success(),
        "litany dispatch worker: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let child = String::from_utf8(out.stdout).unwrap().trim().to_owned();
    assert!(
        child.starts_with(&format!("{parent}-")),
        "hyphenated descent (§2.3): {child} must extend {parent}"
    );
    poll_for_path(&dest, &step_response(&dest, &child));

    // Two live executors, two distinct process groups (§2.9) — the
    // precondition both tests below discriminate on.
    let parent_pgid = holder_pgid(&dest, &parent).expect("parent executor holds its inbox lock");
    let child_pgid = holder_pgid(&dest, &child).expect("child executor holds its inbox lock");
    assert_ne!(
        parent_pgid, child_pgid,
        "every executor takes its own process group, child alike (§2.9)"
    );

    Family {
        _server: server,
        _tmp: tmp,
        dest,
        parent,
        child,
        prompt,
        child_pgid,
    }
}

/// `--stop-children` folds the descendant's group into the sweep: the
/// child dies with the parent, leaving the §2.9 missing-`end` signature
/// and the bl-5156 stopped deposit behind.
#[test]
fn stop_children_fells_the_live_child_executor() {
    let mut fam = live_family();

    run_stop(&fam.dest, &fam.parent, true);

    // The parent took the §2.9 step-3 exit: SIGTERM mid-model-call with a
    // stop pending is the stop, deposited (a root no-op) and exited 0.
    let status = reap(&fam.dest, &mut fam.prompt);
    assert!(
        status.success(),
        "the stopped parent must exit cleanly (§2.9 step 3), got {status:?}"
    );

    // The child's group was signalled too — its executor is gone. Nothing
    // else in this test can end it: its mock answer is minutes away.
    poll_until_no_holder(&fam.dest, &fam.child);

    // §2.9 signature on the child's *own* step record.
    assert_no_terminal_end(&step_response(&fam.dest, &fam.child));

    // The child is not a root, so bl-5156's "deposits, then exits
    // cleanly" is observable: a `stopped` result message from the child,
    // sender-namespaced, in the parent's inbox (§2.6, §2.11).
    let deposited = inbox_dir(&fam.dest, &fam.parent).join(format!("{}-001.md", fam.child));
    poll_for_path(&fam.dest, &deposited);
    let body = fs::read_to_string(&deposited).unwrap();
    assert!(
        body.contains("epitaph: stopped") && body.contains(&format!("from: {}", fam.child)),
        "the felled child deposits its stopped result (§2.9 step 3): {body}"
    );
}

/// A bare `litany stop` stops at the agent boundary: the parent dies, the
/// child keeps running. Without this the cascade could be a kernel-group
/// side effect rather than the opt-in CLI-level walk §2.9 specifies.
#[test]
fn bare_stop_leaves_the_live_child_running() {
    let mut fam = live_family();

    run_stop(&fam.dest, &fam.parent, false);

    let status = reap(&fam.dest, &mut fam.prompt);
    assert!(
        status.success(),
        "the stopped parent must exit cleanly (§2.9 step 3), got {status:?}"
    );
    poll_until_no_holder(&fam.dest, &fam.parent);

    // The sweep is over — `litany stop` returned and the parent has been
    // reaped — so any signal the child was going to receive has been
    // delivered. It is still driving its branch.
    assert_eq!(
        holder_pgid(&fam.dest, &fam.child),
        Some(fam.child_pgid),
        "a bare stop must not cross into the child's group (§2.9)"
    );

    // Teardown: the child outlives its parent by design, so fell it
    // explicitly rather than leaving a detached executor pointed at a
    // temp workspace that is about to be deleted.
    run_stop(&fam.dest, &fam.child, false);
    poll_until_no_holder(&fam.dest, &fam.child);
}
