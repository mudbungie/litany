//! End-to-end proof that a **replayed workspace is drivable** (ARCH
//! §9.2): bundle a live run, replay it into an isolated scratch
//! workspace, and drive the scratch with the ordinary verbs — `litany
//! prompt` forks a fresh root off the config lineage that rode the
//! bundle, and `litany message` + the launched driver advance the
//! replayed agent on its governing config commit (§2.2).
//!
//! Without the governing lineage in the bundle the scratch repo names no
//! `config/*` ref, so `prompt` dies at `rev-parse refs/heads/config/default`
//! and `advance` at "no config/* ancestor" — the §9.2 archival story
//! void. These tests are that regression, at the verb level.
//!
//! The wire is the shared `httpmock` fixture (§4.4): real `bz`, real
//! git, one canned Anthropic SSE stream per model call.

use httpmock::Method::POST;
use httpmock::MockServer;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

use super::poll;
use super::prompt_end_to_end::{
    HAPPY_SSE, scaffold_repo, write_brazen_config, write_global_models,
};

fn litany_bin() -> PathBuf {
    crate::test_support::litany_binary()
}

/// The harness root, brazen config, and workspace every case starts
/// from: a scaffolded workspace whose roles point at the mock row.
struct Fixture {
    holder: TempDir,
    harness: PathBuf,
    brazen_config: PathBuf,
    ws: PathBuf,
}

fn fixture(endpoint: &str) -> Fixture {
    let holder = TempDir::new().unwrap();
    let harness = holder.path().join("harness");
    fs::create_dir_all(&harness).unwrap();
    write_global_models(&harness);
    let brazen_config = write_brazen_config(holder.path(), endpoint);
    let ws = holder.path().join("ws");
    scaffold_repo(&ws, &harness);
    Fixture {
        holder,
        harness,
        brazen_config,
        ws,
    }
}

impl Fixture {
    /// Run a `litany` verb with the harness root and the mock wire wired
    /// in, asserting success and returning trimmed stdout.
    fn litany(&self, args: &[&str]) -> String {
        let out = Command::new(litany_bin())
            .args(args)
            .env("LITANY_HOME", &self.harness)
            .env("BRAZEN_CONFIG", &self.brazen_config)
            .output()
            .expect("spawn litany");
        assert!(
            out.status.success(),
            "litany {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    /// One live run, bundled and replayed: returns the scratch
    /// workspace and the agent id it carries.
    fn run_bundle_replay(&self) -> (PathBuf, String) {
        let agent = self.litany(&["prompt", self.ws.to_str().unwrap(), "ping"]);
        let arch = self.holder.path().join("arch");
        self.litany(&[
            "bundle",
            self.ws.to_str().unwrap(),
            &agent,
            arch.to_str().unwrap(),
        ]);
        let scratch = self.litany(&["replay", arch.to_str().unwrap()]);
        (PathBuf::from(scratch), agent)
    }
}

/// Poll for `path` — the driver runs detached, so the test observes disk
/// exactly like a frontend (§3.5). Bounded by [`poll`]'s silence, not by
/// wall time: the driver may take as long as the box makes it take.
fn wait_for(workspace: &Path, path: &Path) {
    if poll::until(workspace, || path.exists().then_some(())).is_none() {
        panic!(
            "{path:?} never appeared, and {} went untouched for {:?} — nothing is driving it",
            workspace.display(),
            poll::patience()
        );
    }
}

fn mock_server() -> MockServer {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(POST).path("/v1/messages");
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(HAPPY_SSE);
    });
    server
}

#[test]
fn a_replayed_workspace_takes_a_fresh_prompt() {
    let server = mock_server();
    let fx = fixture(&server.base_url());
    let (scratch, archived) = fx.run_bundle_replay();

    // The governing lineage rode the bundle: the scratch repo names the
    // config branch the run forked off (§9.2).
    let scratch_repo = scratch.join("repo.git");
    let branches = crate::template::GitRunner::run_capture(
        &crate::template::RealGit::new(),
        &scratch_repo,
        &["branch", "--list", "--format=%(refname:short)"],
    )
    .unwrap();
    assert!(branches.contains("config/default"), "{branches}");
    assert!(branches.contains(&archived), "{branches}");

    // A fresh root forks off that head and drives to a final response —
    // the replayed workspace is an ordinary workspace (replay is not a
    // mode, §2.3).
    let fresh = fx.litany(&["prompt", scratch.to_str().unwrap(), "ping again"]);
    assert_ne!(fresh, archived);
    let transcript = scratch.join("agents").join(&fresh).join("messages");
    assert!(transcript.join("001-user.md").exists());
    assert!(
        fs::read_dir(&transcript)
            .unwrap()
            .filter_map(Result::ok)
            .any(|e| e.file_name().to_string_lossy().starts_with("002-")),
        "the fresh root landed no assistant entry"
    );
}

#[test]
fn a_replayed_agent_advances_on_its_governing_config() {
    let server = mock_server();
    let fx = fixture(&server.base_url());
    let (scratch, agent) = fx.run_bundle_replay();
    let git = crate::template::RealGit::new();

    // The derivation the driver runs: identical in the scratch and in
    // the workspace it came from (§2.2 — same computation, same
    // candidate set, same commit).
    let source_gov =
        crate::workspace::governing_config(&fx.ws, &crate::workspace::agent_ref(&agent), &git)
            .unwrap();
    let scratch_gov =
        crate::workspace::governing_config(&scratch, &crate::workspace::agent_ref(&agent), &git)
            .unwrap();
    assert_eq!(source_gov, scratch_gov);

    // And the verb-level proof: a message into the replayed agent's
    // inbox launches a driver that delivers and steps — the hop that
    // used to die with "no config/* ancestor for agents/<id>".
    fx.litany(&["message", scratch.to_str().unwrap(), &agent, "again"]);
    let transcript = scratch.join("agents").join(&agent).join("messages");
    wait_for(&scratch, &transcript.join("003-user.md"));
    let landed = poll::until(&scratch, || {
        fs::read_dir(&transcript)
            .unwrap()
            .filter_map(Result::ok)
            .any(|e| e.file_name().to_string_lossy().starts_with("004-"))
            .then_some(())
    });
    assert!(
        landed.is_some(),
        "no step landed on the replay, and the scratch went untouched for {:?}",
        poll::patience()
    );
}
