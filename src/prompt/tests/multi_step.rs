//! Multi-step exchange-loop tests. Drives the loop through
//! [`StubToolExecutor`] to assert §2.5 pairing, per-step on-disk shape,
//! and the `Finish{!ToolUse}` termination rule over brazen `v=1` events.

use super::fixtures::*;
use crate::prompt::run;
use brazen::FinishReason;
use serde_json::{Value, json};

fn tool_use_stream(id: &str, name: &str, cmd_key: &str, cmd_val: &str) -> Vec<u8> {
    stream_of(
        FinishReason::ToolUse,
        &[Block::ToolUse {
            id,
            name,
            input: json!({ cmd_key: cmd_val }),
        }],
    )
}

pub(super) fn final_stream() -> Vec<u8> {
    stream_of(FinishReason::Stop, &[Block::Text("done")])
}

fn last_line_type(bytes: &[u8]) -> String {
    let lines = parse_jsonl(bytes);
    lines.last().unwrap()["type"].as_str().unwrap().to_string()
}

fn finish_reason(bytes: &[u8]) -> String {
    parse_jsonl(bytes)
        .into_iter()
        .find(|e| e["type"] == "finish")
        .unwrap()["reason"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn loop_runs_two_steps_when_first_completion_is_tool_use() {
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let harness = scaffold_harness_root();
    let r1 = tool_use_stream("toolu_01", "bash", "cmd", "ls");
    let r2 = final_stream();
    // Two model calls, and a version guard before each: config resolves
    // at every step boundary (bl-e580), so step 2's load-time guard
    // (§4.4) runs again with it.
    let adapter = StubAdapter::scripted([
        StubAdapter::reply_ok(&version_line()),
        StubAdapter::reply_ok(&r1),
        StubAdapter::reply_ok(&version_line()),
        StubAdapter::reply_ok(&r2),
    ]);
    let git = StubGit::ok();
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let (sleeper, tool_executor) = (
        StubSleeper::default(),
        StubToolExecutor::with_reply("bash", "files: a b"),
    );

    let branch = run(
        repo.path(),
        "list files",
        None,
        None,
        None,
        crate::prompt::PinnedDocs::none(),
        None,
        &valid_deps(
            &adapter,
            &sleeper,
            &git,
            &clock,
            &id,
            &tool_executor,
            harness.path(),
        ),
    )
    .unwrap();
    assert_eq!(branch, "ct-1-deadbeef");
    let worktree = worktree_path(repo.path());

    // Executor saw one call in step 1 with the emitted tool_use.
    let tool_calls = tool_executor.invocations.borrow().clone();
    assert_eq!(tool_calls.len(), 1);
    let (step_dir, tid, name, input) = &tool_calls[0];
    assert_eq!(step_dir, &repo.path().join("steps/ct-1-deadbeef/001"));
    assert_eq!(
        (tid.as_str(), name.as_str(), &input["cmd"]),
        ("toolu_01", "bash", &json!("ls"))
    );

    assert!(!worktree.join("steps").exists());
    let step1_dir = repo.path().join("steps/ct-1-deadbeef/001");
    let step2_dir = repo.path().join("steps/ct-1-deadbeef/002");

    // Step 1 request: one user message, the front-door delivery of the
    // initial message (§2.11) — its deposit frontmatter travels with the
    // body and is model-visible (`deposited_at` is the first clock tick).
    let req1: Value =
        serde_json::from_slice(&std::fs::read(step1_dir.join("request.json")).unwrap()).unwrap();
    assert_eq!(req1["messages"].as_array().unwrap().len(), 1);
    assert_eq!(
        req1["messages"][0]["content"][0]["text"],
        "---\nfrom: user\ndeposited_at: iso-1\n---\nlist files"
    );

    // Step 2 request: §2.5 pairing — assistant tool_use + tool-side
    // tool_result (canonical `Role::Tool`, whose content is a canonical
    // `Content` array; the provider protocol projects the role, §2.3).
    let req2: Value =
        serde_json::from_slice(&std::fs::read(step2_dir.join("request.json")).unwrap()).unwrap();
    let msgs = req2["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 3);
    assert_eq!(msgs[1]["role"], "assistant");
    assert_eq!(msgs[1]["content"][0]["id"], "toolu_01");
    assert_eq!(msgs[2]["role"], "tool");
    assert_eq!(msgs[2]["content"][0]["tool_use_id"], "toolu_01");
    assert_eq!(msgs[2]["content"][0]["content"][0]["text"], "files: a b");

    assert!(worktree.join("goal.md").exists());
    // Step 2's response terminal is brazen's `end`; the finish reason
    // is `stop`.
    let resp2 = std::fs::read(step2_dir.join("response.json")).unwrap();
    assert_eq!(last_line_type(&resp2), "end");
    assert_eq!(finish_reason(&resp2), "stop");

    // Git op log: 11 (the start's preamble — the fork-point lineage query
    // and the settle-the-name living-names scan, §2.3; then control
    // resolution, §2.2 — the `config/*` head enumeration and merge-base,
    // the followed-tip enumeration and its containment merge-base
    // (§2.2, bl-403b),
    // plus five `show` reads, `version` first for the §10 schema-version
    // guard, manifest.yaml before the soul, §5.2)
    // + 10 (step 1 setup: spawn, control rm, the descriptor
    // derivation's five ops — four `cat-file -e` existence reads against
    // the governing config commit and one `checkout`, §3.3 — the
    // settled-name stage (§2.3), add, commit) + 1 (step-1 drain
    // stray-probe) + 2 (user-message delivery add+commit) + 1 (step 1
    // rev-parse) + 2 (step-1 model-output transcript entry add+commit)
    // + 1 (the tool window's unconditional hold-mark probe, §3.3 *Tool
    // control*) + 2 (the tool transcript entry add+commit) + 12 (the step-2
    // boundary's own config resolution — bl-e580: the config question is
    // asked again here, because a `litany config` edit or a `litany
    // workflow` mark that landed during step 1 governs step 2, so the
    // lineage enumeration and containment merge-base run again (4), the
    // role is derived from the branch this time rather than defaulted for
    // a fork (1), and `version`/`providers.yaml`, the workflow-mark
    // probe, the mark's own §10 guard, `workflow.yaml`, `manifest.yaml`
    // and the soul are read again (7 — `StubGit` answers every
    // `rev-parse` with a sha, so the mark probe reads as a standing mark
    // and pays a second `version` read a live unmarked agent does not))
    // + 1 (step-2 drain stray-probe) + 1 (step 2 rev-parse) + 2 (step-2
    // model-output entry add+commit) = 46. The terminal result deposit
    // adds none: the
    // last prompter is `user` (the on-ramp message), so the reply
    // addresses no inbox and neither the branch-tip read nor the returned
    // mark runs (§2.6). Merge-back is gone. The version guard runs no git.
    let runs = git.runs.borrow();
    assert_eq!(runs.len(), 46);
    assert_eq!(runs[18].1, vec!["add", "name"]);
    assert_eq!(runs[19].1, vec!["add", "goal.md", "soul.md"]);
    assert!(runs[20].1[2].contains("step 001: dispatch"));
    // Step-1 drain (§2.11): stray-probe, then the initial user message (001).
    assert_eq!(runs[21].1, vec!["status", "--porcelain", "--", "messages"]);
    assert_eq!(runs[22].1, vec!["add", "messages/001-user.md"]);
    assert!(runs[23].1[2].contains("transcript 001: user"));
    assert_eq!(runs[24].1, vec!["rev-parse", "HEAD"]);
    // Step 1's transcript: model-output entry (002), then the tool result
    // entry (003) — the §2.3 ordering (model output before its tool
    // results). Counters are max-present-plus-one from the messages/
    // listing, so they never collide with the step number. The model
    // output's origin token is the authoring model id (§2.3).
    assert_eq!(runs[25].1, vec!["add", "messages/002-claude-sonnet-5.json"]);
    assert!(runs[26].1[2].contains("transcript 002: claude-sonnet-5"));
    // The tool window opens with the unconditional hold-mark probe
    // (§3.3 *Tool control* — the mark, not the config, asserts a park).
    assert_eq!(runs[27].1[..2], ["cat-file", "blob"]);
    // A tool commit stages the whole worktree (`git add -A`, §2.3) so any
    // worktree side effect the tool produced lands with its result entry.
    assert_eq!(runs[28].1, vec!["add", "-A"]);
    assert!(runs[29].1[2].contains("transcript 003: tool"));
    // Step 2 opens by resolving config at its own boundary (bl-e580):
    // the governing-lineage enumeration, this branch's role, and the
    // control reads — the same `resolve_worker` a `litany advance` hop
    // makes, against this agent's own ref rather than the fork point.
    assert_eq!(runs[30].1[..2], ["for-each-ref", "--format=%(refname)"]);
    assert_eq!(runs[31].1[..2], ["merge-base", "agents/ct-1-deadbeef"]);
    assert_eq!(runs[34].1[0], "log");
    assert_eq!(
        runs[41].1[1],
        "cafecafecafecafecafecafecafecafecafecafe:souls/worker.md"
    );
    // Then its boundary drain (empty inbox → stray-probe only), the
    // branch-tip capture (advanced by step 1's transcript commits), and
    // its own model-output entry (004).
    assert_eq!(runs[42].1, vec!["status", "--porcelain", "--", "messages"]);
    assert_eq!(runs[43].1, vec!["rev-parse", "HEAD"]);
    assert_eq!(runs[44].1, vec!["add", "messages/004-claude-sonnet-5.json"]);
    assert!(runs[45].1[2].contains("transcript 004: claude-sonnet-5"));
    // The terminal result deposit is one structural no-op — the operator
    // prompted this agent, so its reply addresses no inbox (§2.6) — so
    // the entry commit is the last git op and no merge-back follows.

    // The tool entry on disk is the canonical tool_result block.
    let tool_entry = worktree.join("messages/003-tool.json");
    let blocks: Value = serde_json::from_slice(&std::fs::read(&tool_entry).unwrap()).unwrap();
    assert_eq!(blocks[0]["type"], "tool_result");
    assert_eq!(blocks[0]["tool_use_id"], "toolu_01");
    assert_eq!(blocks[0]["content"][0]["text"], "files: a b");
}

#[test]
fn loop_runs_three_steps_when_two_completions_in_a_row_are_tool_use() {
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let harness = scaffold_harness_root();
    let r1 = tool_use_stream("toolu_01", "bash", "cmd", "ls");
    let r2 = tool_use_stream("toolu_02", "bash", "cmd", "pwd");
    let r3 = final_stream();
    // One version guard per step boundary (bl-e580, above).
    let adapter = StubAdapter::scripted([
        StubAdapter::reply_ok(&version_line()),
        StubAdapter::reply_ok(&r1),
        StubAdapter::reply_ok(&version_line()),
        StubAdapter::reply_ok(&r2),
        StubAdapter::reply_ok(&version_line()),
        StubAdapter::reply_ok(&r3),
    ]);
    let git = StubGit::ok();
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let (sleeper, tool_executor) = (StubSleeper::default(), StubToolExecutor::ok());

    run(
        repo.path(),
        "go",
        None,
        None,
        None,
        crate::prompt::PinnedDocs::none(),
        None,
        &valid_deps(
            &adapter,
            &sleeper,
            &git,
            &clock,
            &id,
            &tool_executor,
            harness.path(),
        ),
    )
    .unwrap();

    let step3_resp = repo.path().join("steps/ct-1-deadbeef/003/response.json");
    assert!(step3_resp.exists());
    assert!(!repo.path().join("steps/ct-1-deadbeef/004").exists());

    let invocations = tool_executor.invocations.borrow().clone();
    assert_eq!(invocations.len(), 2);
    assert!(invocations[0].0.ends_with("steps/ct-1-deadbeef/001"));
    assert!(invocations[1].0.ends_with("steps/ct-1-deadbeef/002"));
    assert_eq!(invocations[0].1, "toolu_01");
    assert_eq!(invocations[1].1, "toolu_02");
}

// Loop-termination cases (tool-executor failure) live in
// [`super::multi_step_terminal`].

mod emission_order;
