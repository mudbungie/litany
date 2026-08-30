//! End-to-end proof of the start's fork point through the real binary
//! (ARCH §2.3 *Any ref is a legal fork point*, §7.2 fork-from-history,
//! §3.4 the CLI is the control plane).
//!
//! Two arguments, one fact: the ref a fresh root forks off. `--from`
//! takes any ref — here a *historical commit of a running agent*, the
//! recovery ARCH names and the counterfactual §7.2 describes — and
//! `--config` takes a lineage name, which is how a second config
//! lineage becomes startable at all. Both are proven where it counts:
//! the new branch's ancestry (provenance is the ancestry, §7.2, so the
//! graph is the assertion) and its governing config commit (the fork is
//! the freeze, §2.2).

use super::prompt_end_to_end::{
    HAPPY_SSE, scaffold_repo, write_brazen_config, write_global_models,
};
use httpmock::Method::POST;
use httpmock::MockServer;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tempfile::TempDir;

/// A workspace with a live mock provider: `(holder, workspace, harness,
/// brazen config)`. The mock answers every model call with the same
/// happy stream, so a test's subject is the git shape, never the reply.
struct Fixture {
    _holder: TempDir,
    _server: MockServer,
    ws: PathBuf,
    harness: PathBuf,
    brazen_config: PathBuf,
}

fn fixture() -> Fixture {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(POST).path("/v1/messages");
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(HAPPY_SSE);
    });
    let holder = TempDir::new().unwrap();
    let harness = holder.path().join("harness");
    fs::create_dir_all(&harness).unwrap();
    write_global_models(&harness);
    let brazen_config = write_brazen_config(holder.path(), &server.base_url());
    let ws = holder.path().join("conv");
    scaffold_repo(&ws, &harness);
    Fixture {
        _holder: holder,
        _server: server,
        ws,
        harness,
        brazen_config,
    }
}

impl Fixture {
    /// `litany prompt <ws> <message> [args…]` through the exec binding.
    fn prompt(&self, message: &str, args: &[&str]) -> std::process::Output {
        Command::new(crate::test_support::litany_binary())
            .arg("prompt")
            .arg(&self.ws)
            .arg(message)
            .args(args)
            .env("LITANY_HOME", &self.harness)
            .env("BRAZEN_CONFIG", &self.brazen_config)
            .stderr(Stdio::piped())
            .output()
            .expect("spawn litany prompt")
    }

    /// The agent id a successful start printed (§2.3 — the verb's one
    /// product).
    fn start(&self, message: &str, args: &[&str]) -> String {
        let out = self.prompt(message, args);
        assert!(
            out.status.success(),
            "litany prompt {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    fn bare(&self) -> PathBuf {
        self.ws.join("repo.git")
    }
}

/// The `system[0]` text of an agent's step-1 request — the system slot
/// as it went on the wire (§2.3, §2.8, §4.4 typed request).
fn system_slot(fx: &Fixture, agent: &str) -> String {
    let path = fx.ws.join("steps").join(agent).join("001/request.json");
    let request: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    request["system"][0]["text"].as_str().unwrap().to_string()
}

fn git(dest: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dest)
        .args(args)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .output()
        .expect("spawn git");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

fn git_ok(dest: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(dest)
        .args(args)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .output()
        .expect("spawn git")
        .status
        .success()
}

#[test]
fn a_start_forks_from_any_historical_commit_and_inherits_its_tree() {
    let fx = fixture();
    // Named, so the fork's own name fact is observable below.
    let first = fx.start("ping", &["--name", "pale-otter"]);
    let bare = fx.bare();
    let first_ref = format!("agents/{first}");

    // A *historical* commit of the running agent — its dispatch commit,
    // not its tip — is the fork point. This is §7.2's counterfactual and
    // the recovery ARCH names for the one non-replayable state.
    let on_branch = git(
        &bare,
        &[
            "rev-list",
            "--reverse",
            &first_ref,
            "--not",
            "config/default",
        ],
    );
    // The commit *after* the dispatch commit: the first agent's opening
    // message has landed as a transcript entry, and its model output has
    // not — a mid-conversation commit, neither the branch's start nor
    // its tip.
    let history = on_branch
        .lines()
        .nth(1)
        .expect("the delivery commit of the agent's opening message")
        .to_string();
    assert_ne!(
        history,
        git(&bare, &["rev-parse", &first_ref]),
        "the fork point is history, not the tip"
    );
    let second = fx.start("again", &["--from", &history]);
    assert_ne!(second, first, "a fork from history is a new root agent");
    let second_ref = format!("agents/{second}");

    // Provenance is the ancestry (§7.2): no prefix marks the fork, the
    // graph does.
    assert!(
        git_ok(
            &bare,
            &["merge-base", "--is-ancestor", &history, &second_ref]
        ),
        "the fork point must be an ancestor of the new branch"
    );
    // It is a *root*: 25-character id, no descent, so no parent inbox
    // and no return address (§2.3, §2.6).
    assert_eq!(second.len(), 25, "got {second:?}");

    // The tree came with it (§2.3 *Fork and inheritance*): the first
    // agent's transcript is in the new branch's tree, and the new
    // start's own message continues that sequence rather than restarting
    // it — the counter is max-present-plus-one, derived from the tree.
    let entries = git(&bare, &["ls-tree", "--name-only", &second_ref, "messages/"]);
    assert!(entries.contains("001-user.md"), "got {entries:?}");
    assert!(entries.contains("002-user.md"), "got {entries:?}");
    // What it does *not* inherit is the source's display name (§2.3):
    // the dispatch commit settles `name` unconditionally — and since yog
    // bl-aca4 an omitted name is minted, so a nameless fork of a named
    // agent wears its own minted name rather than being a second agent
    // answering to `pale-otter`.
    assert_eq!(
        git(&bare, &["show", &format!("{first_ref}:name")]),
        "pale-otter"
    );
    let minted = git(&bare, &["show", &format!("{second_ref}:name")]);
    assert_ne!(minted, "pale-otter", "the inherited name never propagates");
    assert!(
        crate::workspace::agent_name::mint::is_minted_shape(&minted),
        "an omitted name is minted as two PascalCase words (bl-79a2), got {minted:?}"
    );
    // And the name reaches the model through the assembled context, not
    // as prose on the user's message (§2.8): each system slot states its
    // agent's own name once, after the goal and before the soul —
    // supplied and minted compose identically.
    let named_system = system_slot(&fx, &first);
    assert!(
        named_system.contains("</goal>\n\nYour name is pale-otter.\n\n"),
        "got {named_system:?}"
    );
    let minted_system = system_slot(&fx, &second);
    assert!(
        minted_system.contains(&format!("</goal>\n\nYour name is {minted}.\n\n")),
        "got {minted_system:?}"
    );
    assert!(minted_system.starts_with("<goal>"), "got {minted_system:?}");
    // The config branch never advanced, and the config commit still
    // governs: the harness-facing control files are absent from the new
    // agent's tree (§2.2, §2.3 step 2).
    assert!(
        !git_ok(&bare, &["cat-file", "-e", &format!("{second_ref}:version")]),
        "control files leave the agent's tree"
    );
}

#[test]
fn a_start_selects_a_config_lineage_by_name() {
    let fx = fixture();
    // A second lineage, forked off the default and advanced with its own
    // worker soul — the config-branch shape §2.2 admits and nothing
    // could start on until now.
    crate::template::authoring::author(
        &fx.ws,
        &fx.ws.join(".no-pools"),
        "strict",
        crate::template::authoring::Origin::Fork { source: "default" },
        |dir| fs::write(dir.join("souls/worker.md"), "You are the strict worker."),
        &crate::template::RealGit::new(),
    )
    .unwrap();

    let agent = fx.start("hi", &["--config", "strict"]);
    let bare = fx.bare();
    let agent_ref = format!("agents/{agent}");
    let strict_head = git(&bare, &["rev-parse", "config/strict"]);

    // Forked off that lineage's head…
    assert!(
        git_ok(
            &bare,
            &["merge-base", "--is-ancestor", &strict_head, &agent_ref]
        ),
        "the named lineage's head must be an ancestor"
    );
    // …and *governed* by it: the pinned soul is the strict lineage's,
    // read from the governing config commit (§2.2, §2.3 step 2). This is
    // the fork-is-the-freeze property, not merely the fork point.
    let soul = git(&bare, &["show", &format!("{agent_ref}:soul.md")]);
    assert_eq!(soul, "You are the strict worker.");
}

#[test]
fn an_unstartable_fork_point_is_declined_before_any_branch_exists() {
    let fx = fixture();
    let bare = fx.bare();
    let before = git(&bare, &["for-each-ref", "--format=%(refname)"]);

    // A lineage the workspace does not have: the decline names the pool.
    let out = fx.prompt("hi", &["--config", "strict"]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(err.contains("no config lineage \"strict\""), "{err}");
    assert!(err.contains("existing lineages: default"), "{err}");

    // A ref nothing answers to.
    let out = fx.prompt("hi", &["--from", "agents/nope"]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(err.contains("no ref or commit \"agents/nope\""), "{err}");

    // Both spellings at once: one start forks off one ref.
    let out = fx.prompt("hi", &["--from", "config/default", "--config", "default"]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(err.contains("not both"), "{err}");

    // Every decline left the workspace exactly as it was.
    assert_eq!(before, git(&bare, &["for-each-ref", "--format=%(refname)"]));
    assert!(!fx.ws.join("agents").exists());
}
