//! `user_message` fires where the drain delivered an operator message
//! (`docs/DESIGN_PROCEDURES.md` §3) — under a version-2 config, never
//! under version 1, on both drivers. Split from [`super`] (the hook seam)
//! along that section line to hold the per-file line cap.

use super::*;

/// One `litany advance` hop over a root whose inbox holds an operator
/// message, under a config at `version` binding `user_message: [notify_ui]`.
fn hop_with_operator_message(version: u32) -> (tempfile::TempDir, std::path::PathBuf) {
    let (h, ws) = fixture::workspace();
    let wt = root_with_dialog(&ws);
    let parts = Parts::new(StubAdapter::scripted([StubAdapter::reply_ok(
        &happy_response_bytes(),
    )]));
    inbox::deposit(
        &ws,
        PARENT,
        inbox::USER_SENDER,
        "again",
        &parts.clock,
        &parts.git,
    )
    .unwrap();
    let cfg = || {
        Ok(WorkerConfig {
            version: Version(version),
            workflow: workflow("events:\n  user_message:\n    - notify_ui\n"),
            ..worker_config()
        })
    };
    let out = run(&ws, PARENT, None, &parts.deps(), &mut { cfg }).unwrap();
    assert!(matches!(out, AdvanceOutcome::Terminal), "{out:?}");
    (h, wt)
}

#[test]
fn user_message_fires_on_the_hop_under_a_version_two_config() {
    let (_h, wt) = hop_with_operator_message(2);
    assert!(notified(&wt, PARENT));
}

#[test]
fn user_message_stays_unfired_under_a_version_one_config() {
    // §3: version 1 keeps the behaviour it always had — the binding is
    // there, and nothing derives the event.
    let (_h, wt) = hop_with_operator_message(1);
    assert!(!notified(&wt, PARENT));
}

#[test]
fn user_message_fires_on_the_root_driver_for_the_opening_prompt() {
    // §3: a root's first prompt is deposited and delivered by the step-1
    // drain, so the general path covers the opening turn. The stub git
    // records the mark's `update-ref` at the agent the exchange minted.
    let repo = scaffold_repo_with_workflow(
        VALID_PER_REPO_PROVIDERS_YAML,
        "events:\n  user_message:\n    - notify_ui\n",
        Some("soul"),
    );
    std::fs::write(repo.path().join("version"), "2\n").unwrap();
    let adapter = StubAdapter::happy(&happy_response_bytes());
    let git = StubGit::ok();
    run_with_stubs(repo.path(), "hello", &adapter, &git).unwrap();
    let marked = git.runs.borrow().iter().any(|(_, argv)| {
        argv.first().map(String::as_str) == Some("update-ref")
            && argv.get(1).map(String::as_str) == Some("refs/litany/notify/ct-1-deadbeef")
    });
    assert!(marked, "{:?}", git.runs.borrow());

    // …and not at version 1, the same exchange otherwise.
    let repo = scaffold_repo_with_workflow(
        VALID_PER_REPO_PROVIDERS_YAML,
        "events:\n  user_message:\n    - notify_ui\n",
        Some("soul"),
    );
    let adapter = StubAdapter::happy(&happy_response_bytes());
    let git = StubGit::ok();
    run_with_stubs(repo.path(), "hello", &adapter, &git).unwrap();
    assert!(
        !git.runs
            .borrow()
            .iter()
            .any(|(_, argv)| argv.first().map(String::as_str) == Some("update-ref")),
        "{:?}",
        git.runs.borrow()
    );
}

#[test]
fn the_shipped_template_titles_the_conversation_end_to_end() {
    // `docs/DESIGN_PROCEDURES.md` §6, over the real shipped config: the
    // operator speaks → the hop fires `user_message` → a titler forks off
    // the tip with the dialog and the titler soul → its one line returns
    // → `titler_return` lands it as the branch's `title`.
    let (_h, ws) = fixture::workspace();
    let wt = root_with_dialog(&ws);
    let template = crate::template::TEMPLATE
        .get_file("workflow.yaml")
        .expect("the template ships workflow.yaml")
        .contents_utf8()
        .expect("utf8");
    let shipped = || {
        Ok(WorkerConfig {
            version: Version(2),
            workflow: workflow(template),
            ..worker_config()
        })
    };

    let parts = Parts::new(StubAdapter::scripted([StubAdapter::reply_ok(
        &happy_response_bytes(),
    )]));
    inbox::deposit(
        &ws,
        PARENT,
        inbox::USER_SENDER,
        "port the parser to the new grammar",
        &parts.clock,
        &parts.git,
    )
    .unwrap();
    let out = run(&ws, PARENT, None, &parts.deps(), &mut { shipped }).unwrap();
    assert!(matches!(out, AdvanceOutcome::Terminal), "{out:?}");
    let titler = parts.launcher.invocations.borrow()[0].clone();
    let titler_wt = agent_worktree(&ws, &titler);
    assert!(
        std::fs::read_to_string(titler_wt.join("goal.md"))
            .unwrap()
            .starts_with("You are the titler for branch")
    );
    assert!(
        std::fs::read_to_string(titler_wt.join("soul.md"))
            .unwrap()
            .contains("one line of at most six words")
    );
    assert!(
        titler_wt.join("messages/001-user.md").exists(),
        "it reads the dialog"
    );

    // The titler answers; the next hop lands the line and steps nothing.
    let tip = parts
        .git
        .run_capture(&titler_wt, &["rev-parse", "HEAD"])
        .unwrap();
    inbox::deposit_result(
        &ws,
        PARENT,
        &titler,
        inbox::Epitaph::FinalResponse,
        tip.trim(),
        Some("Parser port to the new grammar"),
        &parts.clock,
        &parts.git,
    )
    .unwrap();
    let quiet = Parts::new(unreachable_adapter());
    let out = run(&ws, PARENT, None, &quiet.deps(), &mut { shipped }).unwrap();
    assert!(matches!(out, AdvanceOutcome::NothingToDo), "{out:?}");
    assert_eq!(
        std::fs::read_to_string(wt.join("title")).unwrap(),
        "Parser port to the new grammar\n"
    );
    assert!(!notified(&wt, PARENT));
}
