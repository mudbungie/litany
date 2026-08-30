//! The dispatch commit derives the agent's descriptor tree from its
//! governing config commit, filtered to the role's grant (ARCH §3.3,
//! §5.1, §2.3 step 2) — wired, at the root path, with the grant *and the
//! commit* the role actually resolved.
//!
//! The unit tests for the derivation itself are
//! `crate::prompt::dispatch::step_commit::descriptors::tests`; this one proves the
//! **wiring**: that `litany prompt`'s dispatch commit passes the resolved
//! `worker` toolset and not some other list. Its fixture grants
//! `[bash, read_file]` (`fixtures::VALID_PER_REPO_PROVIDERS_YAML`) while
//! the inherited tree carries a `message` descriptor pair — the exact
//! shape of the reproduced failure (yog bl-55b1), where an agent found
//! convincing on-disk documentation for a tool its wire array never
//! declared.

use super::fixtures::*;
use super::stubs::STUB_SHA;
use crate::prompt::run;

#[test]
fn the_dispatch_commit_derives_the_tree_to_the_resolved_grant() {
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("system body"));
    let harness = scaffold_harness_root();
    let adapter = StubAdapter::happy(&happy_response_bytes());
    let git = StubGit::ok();
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let (sleeper, tool_executor) = (StubSleeper::default(), StubToolExecutor::ok());

    // The snapshot the fork inherits from the config commit: schemas for
    // a granted tool and an ungranted one, plus a standalone skill no
    // tool claims. Stub git never materializes a worktree, so the tree
    // is laid down where the fork would have put it.
    let worktree = worktree_path(repo.path());
    std::fs::create_dir_all(worktree.join("descriptions/tools")).unwrap();
    std::fs::create_dir_all(worktree.join("descriptions/skills")).unwrap();
    for name in ["bash", "message"] {
        std::fs::write(
            worktree.join(format!("descriptions/tools/{name}.json")),
            "{}",
        )
        .unwrap();
        std::fs::write(
            worktree.join(format!("descriptions/skills/{name}.md")),
            format!("name: {name}\ndescription: does {name} things\n"),
        )
        .unwrap();
    }
    std::fs::write(
        worktree.join("descriptions/skills/notes.md"),
        "name: notes\ndescription: standalone\n",
    )
    .unwrap();

    run(
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

    let runs = git.runs.borrow();
    let prune = runs
        .iter()
        .find(|(_, args)| {
            args.first().is_some_and(|a| a == "rm")
                && args.iter().any(|a| a == "-q")
                && args.iter().any(|a| a.starts_with("descriptions/"))
        })
        .expect("the dispatch commit stages a descriptor prune");

    assert_eq!(
        prune.1,
        vec![
            "rm",
            "-q",
            "--ignore-unmatch",
            "--",
            "descriptions/tools/message.json",
            "descriptions/skills/message.md",
        ],
        "only the ungranted tool's pair leaves: `bash` is granted \
         (fixtures grant [bash, read_file]) and `notes` is a standalone \
         skill no tool claims (§3.3 two wire homes)"
    );

    // And the granted pair is checked out from the resolved config
    // commit — the tree the fork inherited is never the authority
    // (bl-a900), so the same op runs whether or not it already carried
    // the descriptors.
    let checkout = runs
        .iter()
        .find(|(_, args)| args.first().is_some_and(|a| a == "checkout"))
        .expect("the dispatch commit checks the grant out of the config commit");
    assert_eq!(
        checkout.1,
        vec![
            "checkout",
            STUB_SHA,
            "--",
            "descriptions/tools/bash.json",
            "descriptions/skills/bash.md",
            "descriptions/tools/read_file.json",
            "descriptions/skills/read_file.md",
        ]
    );
}
