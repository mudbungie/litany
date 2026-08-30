//! Integration test: cascade. `litany prompt` against a stalling
//! httpmock + `litany stop` → the group SIGTERM kills `bz` (no handler),
//! and the executor catches its own copy, deposits its `stopped` result
//! on the way out, and exits cleanly (ARCH §2.9 step 3 — "Return is not a
//! verb"). response.json is left closed without a terminal `end` (the
//! stop signature, an independent write untouched by the deposit) and the
//! branch is left unmerged.
//!
//! Idempotence + error-path tests live in `tests/stop_idempotence.rs`.

use super::stop_common::{
    HAPPY_SSE, amend_config, git_command, litany_bin, poll_for_conv_branch_with_diag,
    poll_for_path, reap, repo_git, scaffold_repo, spawn_prompt, write_brazen_config,
    write_global_models,
};
use httpmock::Method::POST;
use httpmock::MockServer;
use std::fs;
use std::process::{Command, Stdio};
use std::time::Duration;
use tempfile::TempDir;

#[test]
fn stop_cascades_sigterm_and_leaves_response_without_terminal_end() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(POST).path("/v1/messages");
        // Long hold — `bz` blocks on the HTTP response while the
        // executor holds its inbox-directory lock fd (§2.11). `litany
        // stop` discovers the pid by that lock fd (§2.9) and cuts the
        // cord; the open (empty) response.json is left without a
        // terminal `end` as the on-disk stop signature. The hold is
        // sized far past the evidence-anchored work between the polls
        // below and the stop (the sibling `stop_children.rs` margin):
        // a hold the test's own progress can outrun under load would
        // deliver HAPPY_SSE and fail the run on machine load, not code.
        then.status(200)
            .header("content-type", "text/event-stream")
            .delay(Duration::from_secs(120))
            .body(HAPPY_SSE);
    });

    let holder = TempDir::new().unwrap();
    let harness = holder.path().join("harness");
    fs::create_dir_all(&harness).unwrap();
    write_global_models(&harness);
    let brazen_config = write_brazen_config(holder.path(), &server.base_url());
    let dest = holder.path().join("conv");
    scaffold_repo(&dest, &harness);

    let mut prompt_child = spawn_prompt(&dest, &harness, &brazen_config, "ping");

    let branch = poll_for_conv_branch_with_diag(&dest, &mut prompt_child);
    let step_dir = dest.join("steps").join(&branch).join("001");
    poll_for_path(&dest, &step_dir.join("response.json"));

    let stop_out = Command::new(litany_bin())
        .arg("stop")
        .arg(&dest)
        .arg(&branch)
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .output()
        .expect("spawn litany stop");
    assert!(
        stop_out.status.success(),
        "litany stop: {}",
        String::from_utf8_lossy(&stop_out.stderr)
    );

    // §2.9 step 3: the executor catches SIGTERM, deposits its result on
    // the way out, and exits cleanly — it does not die on the spot. (This
    // is a root conversation, so the deposit is a structural no-op; the
    // clean exit is the observable.) `bz` still died from its own copy of
    // the group SIGTERM, leaving the missing-`end` signature below.
    let prompt_status = prompt_child.wait().expect("reap litany prompt");
    assert!(
        prompt_status.success(),
        "litany prompt must exit cleanly after depositing on stop, got {prompt_status:?}"
    );

    // §2.9 on-disk signature: latest response.json closed and either
    // empty or whose last JSONL line is not the terminal `end`.
    let resp_path = step_dir.join("response.json");
    let resp = fs::read(&resp_path).unwrap();
    let lines: Vec<&[u8]> = resp
        .split(|b| *b == b'\n')
        .filter(|l| !l.is_empty())
        .collect();
    if let Some(last) = lines.last() {
        let v: serde_json::Value = serde_json::from_slice(last).expect("trailing line is JSON");
        assert_ne!(
            v["type"].as_str(),
            Some("end"),
            "stopped response.json must not end with a terminal `end`; last: {v}"
        );
    }

    // The branch persists as its agents/* ref (§2.3) — there is no
    // `main` and nothing merges; the ref is the record.
    let branch_ref = format!("refs/heads/agents/{branch}");
    let ref_check = git_command(&repo_git(&dest), &["rev-parse", "--verify", &branch_ref])
        .status()
        .expect("spawn git rev-parse");
    assert!(ref_check.success(), "agent ref must persist after stop");
}

/// An Anthropic SSE that resolves to a single `bash` tool call running
/// `command` — the tool-execution window this test needs. bz normalizes
/// the `tool_use` content block (`content_block_start` + one
/// `input_json_delta`) into the canonical stream litany records.
fn tool_use_sse(command: &str) -> String {
    let input = serde_json::json!({ "command": command }).to_string();
    let events = [
        (
            "message_start",
            serde_json::json!({"type":"message_start","message":{"id":"msg_tool","model":"claude-sonnet-5","stop_reason":null,"content":[],"usage":{"input_tokens":2,"output_tokens":0}}}),
        ),
        (
            "content_block_start",
            serde_json::json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_1","name":"bash","input":{}}}),
        ),
        (
            "content_block_delta",
            serde_json::json!({"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":input}}),
        ),
        (
            "content_block_stop",
            serde_json::json!({"type":"content_block_stop","index":0}),
        ),
        (
            "message_delta",
            serde_json::json!({"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":5}}),
        ),
        ("message_stop", serde_json::json!({"type":"message_stop"})),
    ];
    events
        .iter()
        .map(|(name, data)| format!("event: {name}\ndata: {data}\n\n"))
        .collect()
}

/// §2.9 / §2.11: `litany stop` must land during a *tool-execution
/// window* — the model call for step 1 has closed its `response.json`
/// (terminal `end`) and the executor is running a long tool, so the
/// old `response.json`-fd discovery would find no writer. Discovery via
/// the executor's inbox-directory lock fd (held for the whole loop)
/// still finds the pid, so the stop reaches the harness and its tool.
///
/// The stop landing here follows the *same* terminal sequence as the
/// model-call window (§2.9 step 3): the tool subprocess dies with the
/// group SIGTERM (its `KilledBySignal` read as the stop, not a fault),
/// the `stopped` result is deposited, and the executor exits **cleanly**
/// (exit 0) — not the non-zero crash shape a propagated `KilledBySignal`
/// used to produce. (This is a root, so the deposit is a structural
/// no-op; the clean exit is the observable, as in the model-call test.)
#[test]
fn stop_lands_during_tool_execution_via_inbox_lock_fd() {
    let holder = TempDir::new().unwrap();
    // Marker the slow tool touches once it is actually executing — a
    // deterministic "we are in the tool window" signal (no sleep-race).
    let marker = holder.path().join("tool_running");
    // The sleep only has to outlast the marker-anchored work below
    // (one file read plus one `litany stop`); it is sized far past
    // that so no scheduling stretch can let the tool exit on its own
    // before the stop lands.
    let command = format!("touch {} && sleep 120", marker.display());

    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(POST).path("/v1/messages");
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(tool_use_sse(&command));
    });

    let harness = holder.path().join("harness");
    fs::create_dir_all(&harness).unwrap();
    write_global_models(&harness);
    // Seed the data-root tool pool so `litany new` snapshots the bash
    // schema into `descriptions/tools/`, making the tool composable.
    let pool = harness.join("tools");
    fs::create_dir_all(&pool).unwrap();
    fs::copy(
        concat!(env!("CARGO_MANIFEST_DIR"), "/schemas/tools/bash.json"),
        pool.join("bash.json"),
    )
    .unwrap();
    let brazen_config = write_brazen_config(holder.path(), &server.base_url());
    let dest = holder.path().join("conv");
    scaffold_repo(&dest, &harness);
    // Give the worker the bash tool (stop_common's scaffold declares
    // none) — another config-commit amendment (§2.2).
    amend_config(
        &dest,
        &[(
            "providers.yaml",
            "roles:\n  worker:\n    provider: test\n    model: claude-sonnet-5\n    tools: [bash]\n  compactor:\n    provider: test\n    model: claude-haiku-4-5\n",
        )],
    );

    let mut prompt_child = spawn_prompt(&dest, &harness, &brazen_config, "run a slow tool");

    let branch = poll_for_conv_branch_with_diag(&dest, &mut prompt_child);

    // Wait until the tool is actually running: the marker proves the
    // model call finished (step-1 response.json closed with `end`) and
    // the executor is inside the tool's long sleep.
    poll_for_path(&dest, &marker);

    // Discriminator: step-1 response.json ends with terminal `end`, so a
    // response.json-fd scan finds *no* open writer right now. Only the
    // inbox lock fd can reveal the live executor.
    let resp = fs::read(
        dest.join("steps")
            .join(&branch)
            .join("001")
            .join("response.json"),
    )
    .unwrap();
    let last = resp
        .split(|b| *b == b'\n')
        .rfind(|l| !l.is_empty())
        .expect("response.json has content");
    let v: serde_json::Value = serde_json::from_slice(last).expect("trailing line is JSON");
    assert_eq!(
        v["type"].as_str(),
        Some("end"),
        "step-1 response.json must be closed (fd not open) before the stop"
    );

    let stop_out = Command::new(litany_bin())
        .arg("stop")
        .arg(&dest)
        .arg(&branch)
        .stderr(Stdio::piped())
        .output()
        .expect("spawn litany stop");
    assert!(
        stop_out.status.success(),
        "litany stop: {}",
        String::from_utf8_lossy(&stop_out.stderr)
    );

    // The stop reached the harness (via the lock fd) and its pgid: the
    // executor terminates promptly. Had discovery relied on the closed
    // response.json fd, no signal would have been sent and the harness
    // would still be sleeping.
    let status = reap(&dest, &mut prompt_child);
    // §2.9 step 3: the tool's group-SIGTERM death is classified as the
    // stop, so the executor deposits and exits *cleanly* — not the
    // non-zero exit a propagated `KilledBySignal` fault used to produce.
    assert!(
        status.success(),
        "stop during a tool window must exit cleanly (the stopped-deposit exit, §2.9 step 3), got {status:?}"
    );
}
