//! End-to-end subprocess test for `litany prompt` over the real
//! brazen `bz` data plane (ARCH §4.4). Chains `litany new` (workspace creation) and `litany prompt` (one
//! root conversation). The model call execs real `bz` (§4.4);
//! `BRAZEN_CONFIG` points bz at a fixture provider row whose endpoint
//! is an `httpmock` server returning an Anthropic SSE stream. Env is
//! set on the `litany prompt` subprocess, so tests are race-free.
//! Asserts the branch contract — no terminal compaction (§2.7), the
//! agent persists on its own ref — and the wire shape: a typed canonical
//! request on stdin, `v=1` NDJSON with a terminal `end`.

use httpmock::Method::POST;
use httpmock::MockServer;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use tempfile::TempDir;

fn litany_bin() -> std::path::PathBuf {
    crate::test_support::litany_binary()
}

const INHERITED_GIT_ENV: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_PREFIX",
    "GIT_COMMON_DIR",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
];

fn git_command(dest: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new("git");
    for var in INHERITED_GIT_ENV {
        cmd.env_remove(var);
    }
    cmd.arg("-C").arg(dest).args(args);
    cmd
}

fn git_capture(dest: &Path, args: &[&str]) -> String {
    let out = git_command(dest, args).output().expect("spawn git");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// Global `<harness-root>/models.yaml` (ARCH §4.2) — the shipped shape:
/// no `adapter:` override, no models (bl-35e2). The roles' provider row
/// (`test`) and model ids are the per-repo assignment's alone (§4.3).
pub fn write_global_models(harness: &Path) {
    fs::write(harness.join("models.yaml"), "# no adapter override\n").unwrap();
}

/// A brazen config (§4.4) defining a keyless `test` provider row whose
/// endpoint is the mock server. `auth = "none"` needs no credential, so
/// the harness never sees key material (§4.1).
pub fn write_brazen_config(dir: &Path, endpoint: &str) -> std::path::PathBuf {
    let toml = format!(
        "timeout = 10\n\
         [[provider]]\nname = \"test\"\nbase_url = \"{endpoint}\"\n\
         protocol = \"anthropic_messages\"\nauth = \"none\"\n\
         body_defaults = {{ max_tokens = 64 }}\n"
    );
    let path = dir.join("brazen.toml");
    fs::write(&path, toml).unwrap();
    path
}

/// Amend the config commit's providers.yaml so both roles use the
/// fixture `test` brazen row (§4.3) — control lives in the config
/// lineage (§2.2), so the edit is a config commit, not a loose file.
fn write_per_repo_roles(dest: &Path) {
    let yaml = "\
roles:
  worker:
    provider: test
    model: claude-sonnet-5
    tools: [bash, read_file]
  compactor:
    provider: test
    model: claude-haiku-4-5
";
    // Config-commit amendment (§2.2) over the shipped authoring core.
    crate::template::authoring::author(
        dest,
        &dest.join(".no-pools"),
        "default",
        crate::template::authoring::Origin::Advance,
        |dir| fs::write(dir.join("providers.yaml"), yaml),
        &crate::template::RealGit::new(),
    )
    .unwrap();
}

pub fn scaffold_repo(dest: &Path, harness: &Path) {
    let out = Command::new(litany_bin())
        .arg("new")
        .arg(dest)
        .env("LITANY_HOME", harness)
        .output()
        .expect("spawn litany new");
    assert!(
        out.status.success(),
        "litany new: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    write_per_repo_roles(dest);
}

/// Anthropic-native SSE happy stream; `bz` normalizes to `v=1` events.
pub const HAPPY_SSE: &str = concat!(
    "event: message_start\n",
    "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_e2e\",\"model\":\"claude-sonnet-5\",\"stop_reason\":null,\"content\":[],\"usage\":{\"input_tokens\":2,\"output_tokens\":0}}}\n\n",
    "event: content_block_start\n",
    "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
    "event: content_block_delta\n",
    "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"pong\"}}\n\n",
    "event: content_block_stop\n",
    "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
    "event: message_delta\n",
    "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":1}}\n\n",
    "event: message_stop\n",
    "data: {\"type\":\"message_stop\"}\n\n",
);

#[test]
fn prompt_subcommand_persists_conversation_without_terminal_compaction() {
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
    let dest = holder.path().join("conv");
    scaffold_repo(&dest, &harness);

    let bare = dest.join("repo.git");
    let config_head_before = git_capture(&bare, &["rev-parse", "config/default"]);

    let prompt_out = Command::new(litany_bin())
        .arg("prompt")
        .arg(&dest)
        .arg("ping")
        .env("LITANY_HOME", &harness)
        .env("BRAZEN_CONFIG", &brazen_config)
        .stderr(Stdio::piped())
        .output()
        .expect("spawn litany prompt");
    assert!(
        prompt_out.status.success(),
        "litany prompt: {}",
        String::from_utf8_lossy(&prompt_out.stderr)
    );

    let conv_id = String::from_utf8(prompt_out.stdout)
        .unwrap()
        .trim()
        .to_string();
    assert!(!conv_id.contains('/'), "stdout carries the id: {conv_id:?}");
    assert_eq!(conv_id.len(), 25, "got {conv_id:?}");
    let branch_ref = format!("agents/{conv_id}");

    // The config branch does NOT advance (§2.3 branch advancement: only
    // user config edits move it); the agent persists on its own ref.
    let config_head_after = git_capture(&bare, &["rev-parse", "config/default"]);
    assert_eq!(
        config_head_before, config_head_after,
        "config/default must not advance"
    );

    // The dispatch commit removed the control files from the agent's
    // tree (§2.2): providers.yaml lives in the config commit only.
    let control_on_branch = git_command(&bare, &["show", &format!("{branch_ref}:providers.yaml")])
        .output()
        .expect("spawn git show");
    assert!(
        !control_on_branch.status.success(),
        "control files must leave the agent tree (§2.2)"
    );

    // There is NO terminal compaction (§2.7 — the stage is deleted): a
    // final response does not dispatch a compactor, so the conversation
    // tip is the ordinary last transcript commit (a single parent), never
    // a compaction landing, and no `summary/` lands on the branch.
    let conv_parents = git_capture(&bare, &["log", "-1", "--pretty=%P", &branch_ref]);
    assert_eq!(
        conv_parents.split_whitespace().count(),
        1,
        "conv tip is an ordinary commit, not a terminal-compaction landing (§2.7)"
    );
    let summary_on_branch = git_command(&bare, &["show", &format!("{branch_ref}:summary/001.md")])
        .output()
        .expect("spawn git show");
    assert!(
        !summary_on_branch.status.success(),
        "no terminal-compaction summary lands on a final response (§2.7)"
    );

    // Step records live outside every worktree (§2.2 / §2.3).
    let step_dir = dest.join(format!("steps/{conv_id}/001"));
    let request: serde_json::Value =
        serde_json::from_slice(&fs::read(step_dir.join("request.json")).unwrap()).unwrap();
    // Typed canonical request: user content is a `Content::Text` array,
    // the goal is pinned at the head of `system`, and `stream` is absent
    // (brazen's default governs, §4.4). The initial message reached the
    // request through the front door (§2.11): deposited into the inbox and
    // delivered by the step-1 drain, so its `from:` / `deposited_at:`
    // frontmatter travels with the body and is model-visible by design.
    let user_text = request["messages"][0]["content"][0]["text"]
        .as_str()
        .unwrap();
    assert!(
        user_text.starts_with("---\nfrom: user\n"),
        "got {user_text:?}"
    );
    assert!(user_text.contains("\ndeposited_at: "), "got {user_text:?}");
    assert!(user_text.ends_with("\n---\nping"), "got {user_text:?}");
    assert!(
        request["system"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("<goal>\nping\n</goal>"),
    );
    // litany sets no `stream` (brazen default governs, §4.4); the typed
    // request serializes the unset Option as JSON `null`.
    assert!(request["stream"].is_null());

    // response.json is `v=1` NDJSON; the terminal line is `end`.
    let lines: Vec<serde_json::Value> = fs::read(step_dir.join("response.json"))
        .unwrap()
        .split(|b| *b == b'\n')
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_slice(l).expect("valid JSON line"))
        .collect();
    assert_eq!(lines.first().unwrap()["type"], "message_start");
    assert_eq!(lines.last().unwrap()["type"], "end");
    let text = lines.iter().find(|e| e["type"] == "content_delta").unwrap();
    assert_eq!(text["delta"]["text_delta"], "pong");

    // Step records are never git-tracked (§2.2): nothing under steps/
    // is in the agent branch's tree.
    assert!(
        git_capture(&bare, &["ls-tree", "-r", "--name-only", &branch_ref])
            .lines()
            .all(|l| !l.starts_with("steps/"))
    );
    // The branch ref survives (§2.3); the agent worktree persists under
    // agents/ — quiescence, not teardown (§2.3 step 6, §2.6).
    assert!(dest.join("agents").join(&conv_id).exists());
    assert!(git_capture(&bare, &["branch", "--list", &branch_ref]).contains(&conv_id));
}

#[test]
fn prompt_subcommand_surfaces_missing_repo() {
    let holder = TempDir::new().unwrap();
    let harness = holder.path().join("harness");
    fs::create_dir_all(&harness).unwrap();
    write_global_models(&harness);
    let out = Command::new(litany_bin())
        .arg("prompt")
        .arg(holder.path().join("does-not-exist"))
        .arg("hi")
        .env("LITANY_HOME", &harness)
        .stderr(Stdio::piped())
        .output()
        .expect("spawn litany prompt");
    assert!(!out.status.success(), "expected failure on missing repo");
    assert!(String::from_utf8_lossy(&out.stderr).contains("litany prompt"));
}
