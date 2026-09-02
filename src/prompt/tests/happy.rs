//! Happy-path test: full orchestration with valid inputs, over the
//! brazen `bz` data plane (ARCH §4.4).
//!
//! Asserts the conversation branch is spawned, the dispatch commit lays
//! goal.md + soul.md, the diagnostic step record lands at the conv-repo
//! root outside the worktree (§2.2 / §2.3), the request is a typed
//! canonical request, and the response is brazen `v=1` NDJSON with a
//! terminal `end`.

use super::fixtures::*;
use super::stubs::STUB_SHA;
use crate::prompt::run;
use crate::prompt::step::StepMeta;
use std::ffi::OsStr;

#[test]
fn run_happy_path_writes_branch_worktree_and_two_commits() {
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("system body"));
    let harness = scaffold_harness_root();
    let adapter = StubAdapter::happy(&happy_response_bytes());
    let git = StubGit::ok();
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let (sleeper, tool_executor) = (StubSleeper::default(), StubToolExecutor::ok());

    let branch = run(
        repo.path(),
        "hello",
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
    let repo_git = crate::workspace::repo_git(repo.path());

    let goal = std::fs::read_to_string(worktree.join("goal.md")).unwrap();
    assert_eq!(goal, "hello");
    let soul = std::fs::read_to_string(worktree.join("soul.md")).unwrap();
    assert_eq!(soul, "system body");

    assert!(!worktree.join("steps").exists());
    let step_dir = repo.path().join("steps/ct-1-deadbeef/001");
    let request: serde_json::Value =
        serde_json::from_slice(&std::fs::read(step_dir.join("request.json")).unwrap()).unwrap();
    assert_eq!(request["model"], "claude-sonnet-5");
    // Goal is prepended to the soul and rides as a canonical
    // `Content::Text` in `system[0]` (§2.8, §4.4 typed request).
    let name_file = std::fs::read_to_string(worktree.join("name")).unwrap();
    assert_eq!(
        request["system"][0]["text"].as_str().unwrap(),
        format!(
            "<goal>\nhello\n</goal>\n\nYour name is {}.\n\nsystem body",
            name_file.trim()
        ),
        "the minted name composes into the system slot exactly as a supplied one (§2.8)"
    );
    assert_eq!(request["messages"][0]["role"], "user");
    // The initial user message entered through the front door (§2.11):
    // deposited into the agent's own inbox, then delivered by the step-1
    // drain. Its `from:` / `deposited_at:` frontmatter travels with the
    // file and is model-visible by design (§2.11) — `deposited_at` is the
    // first `now_iso8601` tick (`iso-1`).
    assert_eq!(
        request["messages"][0]["content"][0]["text"],
        "---\nfrom: user\ndeposited_at: iso-1\n---\nhello"
    );
    assert_eq!(request["max_tokens"], 4096);
    // `stream` is not set by litany — brazen's default governs (§4.4).
    // The typed request serializes an unset Option as JSON `null`.
    assert!(request["stream"].is_null());

    // response.json is brazen `v=1` NDJSON: first line message_start,
    // and the terminal line is `{"type":"end"}` (§4.4).
    let lines = parse_jsonl(&std::fs::read(step_dir.join("response.json")).unwrap());
    assert!(lines.len() >= 2, "expected event stream, got {lines:?}");
    assert_eq!(lines.first().unwrap()["type"], "message_start");
    let text = lines
        .iter()
        .find(|e| e["type"] == "content_delta")
        .expect("expected a content_delta");
    assert_eq!(text["delta"]["text_delta"], "hi there");
    assert_eq!(lines.last().unwrap()["type"], "end");
    let finish = lines.iter().find(|e| e["type"] == "finish").unwrap();
    assert_eq!(finish["reason"], "stop");

    // meta.json carries the branch-tip sha at step-start (§2.10). The
    // stub git's revision captures return the fixed stub sha.
    // The deposit's `deposited_at` consumed `iso-1`, so the step-1 model
    // call bookends at `iso-2` / `iso-3`.
    let meta: StepMeta =
        serde_json::from_slice(&std::fs::read(step_dir.join("meta.json")).unwrap()).unwrap();
    assert_eq!(meta.commit, STUB_SHA);
    assert_eq!(meta.started_at, "iso-2");
    assert_eq!(meta.ended_at, "iso-3");

    // Adapter called twice: the version guard (`bz --version`) then the
    // model call (`bz --json --provider anthropic`, request on stdin).
    let invocations = adapter.observed.borrow().clone();
    assert_eq!(invocations.len(), 2, "version guard + model call");
    let (binary, args, stdin) = invocations[0].clone();
    assert_eq!(binary, OsStr::new("bz"));
    assert_eq!(args, vec!["--version"]);
    assert!(stdin.is_empty());
    let (binary, args, stdin) = invocations[1].clone();
    assert_eq!(binary, OsStr::new("bz"));
    assert_eq!(args, vec!["--json", "--provider", "anthropic"]);
    // The model-call stdin matches request.json byte-for-byte in
    // content (pretty-print differs, so compare parsed).
    let wire: serde_json::Value = serde_json::from_slice(&stdin).unwrap();
    assert_eq!(wire, request);

    // Git sequence: 11 (the start's preamble against repo.git — the
    // fork-point lineage query, §2.3; then the settle-the-name
    // pre-flight's living-names scan, §2.3 — the name is minted here,
    // nothing was supplied; then control resolution from the
    // config commit, §2.2: the `config/*` head enumeration and its
    // merge-base — the ancestry derivation of the governing commit —
    // then the followed-tip derivation over it (§2.2, bl-403b): the
    // head-tip enumeration and its containment merge-base — plus
    // five `show` reads — `version` first, the §10 schema-version guard,
    // then providers/workflow/manifest/soul)
    // + 1 (branch spawn off the fork point) + 9 (dispatch commit:
    // control-file removal, the descriptor derivation's four `cat-file
    // -e` existence reads and one `checkout` — §3.3 — the settled-name
    // stage (§2.3, `workspace::agent_name`), then add, commit —
    // §2.3 step 2) + 1 (drain
    // stray-probe, §2.11) + 2 (user-message delivery commit, §2.11) + 1
    // (rev-parse) + 2 (model-output transcript entry add + commit). The
    // terminal result deposit adds none: the last prompter is `user`
    // (the on-ramp message this same drain delivered), so the reply
    // addresses no inbox and neither the branch-tip read nor the
    // returned mark runs (§2.6). Merge-back is gone (§2.6): the root
    // branch persists on its own ref. The version guard runs no git.
    let runs = git.runs.borrow();
    assert_eq!(runs.len(), 27);
    for (dest, _args) in &runs[0..12] {
        assert_eq!(dest, &repo_git, "control + spawn run against repo.git");
    }
    // The start's preamble: the fork point (§2.3) — the lineage pool
    // the default `--config` is checked against — then the ancestry
    // derivation of its governing config commit (§2.2), taken against
    // the fork point itself.
    assert_eq!(runs[0].1[1], "--format=%(refname:short)");
    // The settle-the-name pre-flight's `agents/*` scan (§2.3): the one
    // occupied-set derivation both the supplied and the minted arm read.
    assert_eq!(
        runs[1].1,
        vec![
            "for-each-ref",
            "--format=%(refname:short)",
            "refs/heads/agents/"
        ]
    );
    assert_eq!(runs[2].1[1], "--format=%(refname)");
    assert_eq!(
        runs[3].1,
        vec!["merge-base", "config/default", "refs/heads/config/default"]
    );
    // The follow-the-tip derivation (§2.2, bl-403b): every config head's
    // tip, and the containment check that keeps only lineages standing
    // over the governing commit — one distinct tip is followed.
    assert_eq!(runs[4].1[1], "--format=%(objectname)");
    assert_eq!(
        runs[5].1,
        vec!["merge-base", "--is-ancestor", STUB_SHA, STUB_SHA]
    );
    assert_eq!(runs[6].1, vec!["show", &format!("{STUB_SHA}:version")]);
    assert_eq!(
        runs[7].1,
        vec!["show", &format!("{STUB_SHA}:providers.yaml")]
    );
    assert_eq!(
        runs[8].1,
        vec!["show", &format!("{STUB_SHA}:workflow.yaml")]
    );
    assert_eq!(
        runs[9].1,
        vec!["show", &format!("{STUB_SHA}:manifest.yaml")]
    );
    assert_eq!(
        runs[10].1,
        vec!["show", &format!("{STUB_SHA}:souls/worker.md")]
    );
    let args8 = &runs[11].1;
    assert_eq!(
        args8[..4],
        ["worktree", "add", "-b", "agents/ct-1-deadbeef"]
    );
    assert_eq!(args8[4], worktree.to_string_lossy().to_string());
    assert_eq!(args8[5], "config/default");
    for (dest, _args) in &runs[12..27] {
        assert_eq!(dest, &worktree, "post-spawn git runs inside the worktree");
    }
    // Dispatch commit (§2.3 step 2): the config commit's control files
    // leave the agent's tree (§2.2), then goal + soul commit.
    assert_eq!(
        runs[12].1[..5],
        ["rm", "-r", "-q", "--ignore-unmatch", "--"]
    );
    let removed: Vec<&str> = runs[12].1[5..].iter().map(String::as_str).collect();
    assert_eq!(removed, crate::workspace::CONTROL_PATHS);
    // 12-16 are the descriptor derivation (§3.3): the grant checked
    // against the governing config commit, then checked out of it —
    // asserted arg-for-arg in [`super::descriptor_prune`].
    // 18 stages the settled name (§2.3): the trim's fourth part, always
    // written — and since yog bl-aca4 never empty at creation: this root
    // was started without one, so the pre-flight minted a name.
    assert_eq!(runs[18].1, vec!["add", "name"]);
    let minted = std::fs::read_to_string(worktree.join("name")).unwrap();
    let minted = minted.trim();
    assert!(
        crate::workspace::agent_name::mint::is_minted_shape(minted),
        "a minted name is two PascalCase words (bl-79a2), got {minted:?}"
    );
    assert_eq!(runs[19].1, vec!["add", "goal.md", "soul.md"]);
    assert_eq!(runs[20].1[0], "commit");
    assert!(runs[20].1[2].contains("step 001: dispatch"));
    assert!(runs[20].1[2].contains("[ct-1-deadbeef]"));
    // The step-1 drain (§2.11 *Delivery*): a stray-recovery probe over
    // messages/ (clean here — no add/commit), then the initial user
    // message delivered from the inbox as the first transcript entry,
    // before step 1's read state is captured.
    assert_eq!(runs[21].1, vec!["status", "--porcelain", "--", "messages"]);
    assert_eq!(runs[22].1, vec!["add", "messages/001-user.md"]);
    assert!(runs[23].1[2].contains("transcript 001: user"));
    assert_eq!(runs[24].1, vec!["rev-parse", "HEAD"]);

    // The transcript writer commits the model-output entry (§2.3): the
    // sealed staging file is renamed to messages/002-<model-id>.json —
    // the origin token is the model that authored it (§2.3) — and
    // committed.
    assert_eq!(runs[25].1, vec!["add", "messages/002-claude-sonnet-5.json"]);
    assert_eq!(runs[26].1[0], "commit");
    assert!(runs[26].1[2].contains("transcript 002: claude-sonnet-5"));
    assert!(runs[26].1[2].contains("[ct-1-deadbeef]"));
    // The renamed entry is on disk in the worktree and holds the
    // canonical model-output blocks (the "hi there" text block) plus the
    // provider's own token usage (§2.3 *Usage rides the entry*) — so a transcript reader
    // states real counts from the committed bytes alone.
    let entry = worktree.join("messages/002-claude-sonnet-5.json");
    let committed: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&entry).unwrap()).unwrap();
    assert_eq!(committed["content"][0]["type"], "text");
    assert_eq!(committed["content"][0]["text"], "hi there");
    assert_eq!(
        committed["usage"],
        serde_json::json!({"input_tokens": 5, "output_tokens": 3})
    );
    // The staging file left by rename — no debris under steps/.
    assert!(!step_dir.join("staging.json").exists());

    // The terminal result deposit (§2.6, §2.3 step 5) is one structural
    // no-op here — the operator prompted this agent, so its reply is
    // read in this conversation and addresses no inbox — so the entry
    // commit is the last git op and no merge-back follows.
    assert_eq!(runs.len(), 27);
}
