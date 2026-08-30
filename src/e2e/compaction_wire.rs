//! End-to-end proof that a compaction over a transcript that used tools
//! reaches the wire intact (ARCH §2.7, §3.3), over real `bz` against a
//! fixture provider row.
//!
//! The bl-f021 repro: a compactor forks with the dispatching branch's
//! transcript in its tree (§2.3 *Fork and inheritance*) and the
//! checkpoint clock is read closing a tool step (§6), so the history it
//! inherits ends on a `tool_use` / `tool_result` pair for a tool that is
//! not one of its two. With only `write_summary` / `mark_for_deletion`
//! declared, the provider refused the compactor's very first call
//! (`parse_input`: "tool accepts only text content") and compaction never
//! completed on any branch that had used a tool.
//!
//! The assertion is on the bytes the provider row actually received —
//! the projected wire body `bz` wrote, captured by the fixture server
//! (the `prompt_retry.rs` / `advance_cli.rs` scripted-TCP pattern), not
//! on the harness's own record alone.

use crate::template::GitRunner;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tempfile::TempDir;

use super::prompt_end_to_end::{
    HAPPY_SSE, scaffold_repo, write_brazen_config, write_global_models,
};

fn litany_bin() -> std::path::PathBuf {
    crate::test_support::litany_binary()
}

/// Bodies the fixture provider row received, in arrival order.
type Received = Arc<Mutex<Vec<serde_json::Value>>>;

/// A provider endpoint that records each request body and answers every
/// call with `HAPPY_SSE`. Returns `(base_url, received)`.
fn spawn_recording_server() -> (String, Received) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let received: Received = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&received);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.expect("accept");
            if let Some(body) = read_http_body(&mut stream)
                && let Ok(value) = serde_json::from_slice::<serde_json::Value>(&body)
            {
                sink.lock().unwrap().push(value);
            }
            let resp = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\n\
                 content-length: {}\r\nconnection: close\r\n\r\n{HAPPY_SSE}",
                HAPPY_SSE.len()
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.flush();
        }
    });
    (format!("http://127.0.0.1:{port}"), received)
}

/// Read one HTTP request off `stream` and return its body, using the
/// `content-length` header (the adapter always sends one).
fn read_http_body(stream: &mut TcpStream) -> Option<Vec<u8>> {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if stream.read(&mut byte).ok()? == 0 {
            return None;
        }
        head.push(byte[0]);
    }
    let len: usize = String::from_utf8_lossy(&head)
        .to_lowercase()
        .lines()
        .find_map(|l| {
            l.strip_prefix("content-length:")
                .map(str::trim)
                .map(str::to_owned)
        })?
        .parse()
        .ok()?;
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body).ok()?;
    Some(body)
}

/// A root-shaped agent id (§2.3) for the dispatching branch.
const PARENT: &str = "20260101-p1";

/// Run one git command in `dest`, through the harness's own runner (which
/// scrubs the inherited `GIT_*` hook environment, `crate::template`).
fn git_run(dest: &Path, args: &[&str]) {
    crate::template::RealGit::new()
        .run(dest, args)
        .unwrap_or_else(|e| panic!("git {args:?}: {e}"));
}

/// Fabricate the dispatching branch: an agent worktree forked off the
/// config head, carrying a transcript whose last exchange used `bash` —
/// exactly the shape a checkpoint fires on top of (§6).
fn fabricate_parent_that_used_bash(repo: &Path) {
    let bare = repo.join("repo.git");
    let worktree = repo.join("agents").join(PARENT);
    git_run(
        &bare,
        &[
            "worktree",
            "add",
            "-b",
            &format!("agents/{PARENT}"),
            worktree.to_str().unwrap(),
            "config/default",
        ],
    );
    let messages = worktree.join("messages");
    fs::create_dir_all(&messages).unwrap();
    fs::write(worktree.join("goal.md"), "run a command\n").unwrap();
    fs::write(messages.join("001-user.md"), "run a command\n").unwrap();
    fs::write(
        messages.join("002-claude-sonnet-5.json"),
        serde_json::json!([{
            "type": "tool_use",
            "id": "toolu_bash",
            "name": "bash",
            "input": {"command": "echo hello-from-tool"}
        }])
        .to_string(),
    )
    .unwrap();
    fs::write(
        messages.join("003-tool.json"),
        serde_json::json!([{
            "type": "tool_result",
            "tool_use_id": "toolu_bash",
            "content": [{"type": "text", "text": "hello-from-tool\n"}],
            "is_error": false
        }])
        .to_string(),
    )
    .unwrap();
    git_run(&worktree, &["add", "-A"]);
    git_run(&worktree, &["commit", "-m", "checkpoint"]);
}

/// Poll `path` until its `v=1` NDJSON closes on a terminal `end` (§4.4),
/// driving the branch each round, and return the parsed events.
///
/// Existence is not completion: the harness creates `response.json` and
/// the adapter streams into it, so a test that reads on first sight can
/// see an empty or half-written file. Lines are parsed leniently for the
/// same reason — a torn trailing line is a snapshot artifact, not a
/// defect — and the terminal `end` is the file's own completion signal.
fn wait_for_terminal_end(path: &Path, ctx: &Ctx, deadline: Duration) -> Vec<serde_json::Value> {
    let start = Instant::now();
    loop {
        let lines: Vec<serde_json::Value> = fs::read(path)
            .unwrap_or_default()
            .split(|b| *b == b'\n')
            .filter(|l| !l.is_empty())
            .filter_map(|l| serde_json::from_slice(l).ok())
            .collect();
        if lines.last().is_some_and(|e| e["type"] == "end") {
            return lines;
        }
        assert!(
            start.elapsed() < deadline,
            "the compactor's step never completed: {path:?}"
        );
        ctx.advance();
    }
}

/// Everything needed to drive the compactor branch to its next step.
struct Ctx {
    repo: std::path::PathBuf,
    harness: std::path::PathBuf,
    brazen_config: std::path::PathBuf,
    agent: String,
}

impl Ctx {
    /// Run one `litany advance` in the **foreground**, then pause.
    ///
    /// The front-door dispatch already detach-launched a driver (§2.11),
    /// and normally that one does the work. Driving it here as well costs
    /// nothing and removes the test's dependence on a detached process
    /// surviving a saturated machine: the executor lock (§2.11) admits
    /// exactly one driver, so whichever wins, the branch steps — a lost
    /// acquire is a clean no-op, not a contention failure.
    fn advance(&self) {
        let _ = Command::new(litany_bin())
            .arg("advance")
            .arg(&self.repo)
            .arg(&self.agent)
            .env("LITANY_HOME", &self.harness)
            .env("BRAZEN_CONFIG", &self.brazen_config)
            .output();
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Poll until the fixture row has recorded a request, and return it.
fn wait_for_request(received: &Received, ctx: &Ctx, deadline: Duration) -> serde_json::Value {
    let start = Instant::now();
    loop {
        if let Some(first) = received.lock().unwrap().first() {
            return first.clone();
        }
        assert!(
            start.elapsed() < deadline,
            "the compactor never reached the provider row"
        );
        ctx.advance();
    }
}

#[test]
fn a_compactor_over_a_tool_using_transcript_reaches_the_wire() {
    let (endpoint, received) = spawn_recording_server();
    let holder = TempDir::new().unwrap();
    let harness = holder.path().join("harness");
    fs::create_dir_all(&harness).unwrap();
    write_global_models(&harness);
    let brazen_config = write_brazen_config(holder.path(), &endpoint);
    let repo = holder.path().join("ws");
    scaffold_repo(&repo, &harness);
    fabricate_parent_that_used_bash(&repo);

    // Dispatch the compactor the way the checkpoint does (§2.7): an
    // ordinary child, whose front-door launch drives `litany advance`.
    let out = Command::new(litany_bin())
        .args(["dispatch", "compactor"])
        .arg(&repo)
        .arg(PARENT)
        .env("LITANY_HOME", &harness)
        .env("BRAZEN_CONFIG", &brazen_config)
        .output()
        .expect("spawn litany dispatch compactor");
    assert!(
        out.status.success(),
        "litany dispatch compactor: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let child = String::from_utf8(out.stdout).unwrap().trim().to_string();

    let step_dir = repo.join(format!("steps/{child}/001"));
    let deadline = Duration::from_secs(120);
    let ctx = Ctx {
        repo: repo.clone(),
        harness: harness.clone(),
        brazen_config: brazen_config.clone(),
        agent: child,
    };

    // The adapter seam: the body the provider row actually received,
    // projected by real `bz` from the harness's canonical request. It
    // declares the compactor's own two tools plus the tool its inherited
    // transcript already names (§3.3).
    let request = wait_for_request(&received, &ctx, deadline);
    let names: Vec<&str> = request["tools"]
        .as_array()
        .expect("a tools array reached the wire")
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"write_summary"), "{names:?}");
    assert!(names.contains(&"mark_for_deletion"), "{names:?}");
    assert!(
        names.contains(&"bash"),
        "the inherited transcript's tool must be declared: {names:?}"
    );
    // The inherited exchange rides verbatim — the declaration was widened
    // to fit the history, not the history rewritten to fit the toolset.
    assert!(
        request["messages"].to_string().contains("toolu_bash"),
        "{}",
        request["messages"]
    );

    // The wire held: the stream closed on a terminal `end` with no
    // `parse_input` error — the failure this ball reproduced (§4.4).
    let lines = wait_for_terminal_end(&step_dir.join("response.json"), &ctx, deadline);
    assert!(
        lines.iter().all(|e| e["type"] != "error"),
        "the compactor's first call must not error: {lines:?}"
    );
}
