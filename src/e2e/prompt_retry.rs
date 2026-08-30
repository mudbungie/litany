//! Forced-retry integration (ARCH §12 v0.6 criterion, §2.10, §4.4).
//!
//! Drives real `litany prompt` → real `bz` against a mock endpoint that
//! returns HTTP 529 (overloaded) on the first hit and a clean Anthropic
//! SSE stream on the second. Asserts the harness-owned retry loop
//! re-invoked `bz`: `response.json` carries exactly TWO attempt segments
//! (§4.4), the first an in-band `error` with provider status 529 whose
//! `CanonicalError::retryable()` drove the retry, and the last segment
//! classifies complete. A raw sequencing TCP server is used (httpmock
//! cannot vary a response by hit count within one subprocess run).

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use tempfile::TempDir;

fn litany_bin() -> std::path::PathBuf {
    crate::test_support::litany_binary()
}

/// Env vars that, when inherited, override `-C <repo>` and redirect a
/// child `git` invocation back onto the outer repo (e.g. when this test
/// runs under a git hook, which exports `GIT_DIR`/`GIT_INDEX_FILE` into
/// the environment) — cleared on every spawn. The yog repo's
/// `git_tree::cmd::INHERITED_GIT_ENV` mirrors this list.
const INHERITED_GIT_ENV: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_PREFIX",
    "GIT_COMMON_DIR",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
];

const HAPPY_SSE: &str = concat!(
    "event: message_start\n",
    "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_r\",\"model\":\"claude-sonnet-5\",\"stop_reason\":null,\"content\":[],\"usage\":{\"input_tokens\":2,\"output_tokens\":0}}}\n\n",
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

const OVERLOADED_529: &str =
    r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#;

/// Serve a scripted `(status, content_type, body)` list, one per
/// incoming connection (one `bz` attempt = one process = one
/// connection). Returns the base URL.
fn spawn_seq_server(responses: Vec<(u16, &'static str, String)>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        for (status, ctype, body) in responses {
            let (mut stream, _) = listener.accept().expect("accept");
            drain_http_request(&mut stream);
            let resp = format!(
                "HTTP/1.1 {status} STATUS\r\ncontent-type: {ctype}\r\n\
                 content-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(resp.as_bytes()).expect("write response");
            stream.flush().expect("flush");
        }
    });
    format!("http://127.0.0.1:{port}")
}

/// Read one chunk of `bz`'s HTTP request off the socket. `bz` writes its
/// (small) request in full before reading the response, and the socket
/// send buffer holds it, so a single best-effort read is enough to let
/// its write complete before we reply. Branch-free by construction.
fn drain_http_request(stream: &mut TcpStream) {
    let mut tmp = [0u8; 8192];
    let _ = stream.read(&mut tmp);
}

fn write_global_models(harness: &Path) {
    // The shipped shape (bl-35e2): no `adapter:` override, no models —
    // the roles' provider row and model ids are the assignment's alone.
    fs::write(harness.join("models.yaml"), "# no adapter override\n").unwrap();
}

fn write_brazen_config(dir: &Path, endpoint: &str) -> std::path::PathBuf {
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

fn scaffold(dest: &Path, harness: &Path) {
    let out = Command::new(litany_bin())
        .arg("new")
        .arg(dest)
        .env("LITANY_HOME", harness)
        .output()
        .expect("spawn litany new");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Point the roles at the fixture brazen row — a config-commit
    // amendment (§2.2: control lives in the config lineage), over the
    // shipped authoring core rather than hand-rolled worktree juggling.
    let providers = "\
roles:
  worker:
    provider: test
    model: claude-sonnet-5
  compactor:
    provider: test
    model: claude-haiku-4-5
";
    crate::template::authoring::author(
        dest,
        &dest.join(".no-pools"),
        "default",
        crate::template::authoring::Origin::Advance,
        |dir| fs::write(dir.join("providers.yaml"), providers),
        &crate::template::RealGit::new(),
    )
    .unwrap();
}

#[test]
fn retryable_529_then_clean_writes_two_segments_and_completes() {
    let endpoint = spawn_seq_server(vec![
        (529, "application/json", OVERLOADED_529.to_string()),
        (200, "text/event-stream", HAPPY_SSE.to_string()),
    ]);

    let holder = TempDir::new().unwrap();
    let harness = holder.path().join("harness");
    fs::create_dir_all(&harness).unwrap();
    write_global_models(&harness);
    let brazen_config = write_brazen_config(holder.path(), &endpoint);
    let dest = holder.path().join("conv");
    scaffold(&dest, &harness);

    let out = Command::new(litany_bin())
        .arg("prompt")
        .arg(&dest)
        .arg("ping")
        .env("LITANY_HOME", &harness)
        .env("BRAZEN_CONFIG", &brazen_config)
        .stderr(Stdio::piped())
        .output()
        .expect("spawn litany prompt");
    assert!(
        out.status.success(),
        "litany prompt: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let conv_id = String::from_utf8(out.stdout).unwrap().trim().to_string();

    let response = fs::read(dest.join(format!("steps/{conv_id}/001/response.json"))).unwrap();
    let lines: Vec<serde_json::Value> = response
        .split(|b| *b == b'\n')
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_slice(l).expect("valid JSON line"))
        .collect();

    // Exactly TWO attempt segments (two terminal `end` lines).
    let ends = lines.iter().filter(|e| e["type"] == "end").count();
    assert_eq!(ends, 2, "expected two attempt segments, got {lines:#?}");

    // Segment 1 carries the retryable 529 that drove the retry.
    let err = lines
        .iter()
        .find(|e| e["type"] == "error")
        .expect("first segment carries an error");
    assert_eq!(err["kind"]["provider"]["status"], 529);

    // The last segment completed: a `finish` then the terminal `end`,
    // with the recovered text.
    assert_eq!(lines.last().unwrap()["type"], "end");
    assert!(lines.iter().any(|e| e["type"] == "finish"));
    assert!(
        lines
            .iter()
            .any(|e| e["type"] == "content_delta" && e["delta"]["text_delta"] == "pong")
    );

    // The retry recovered cleanly and the conversation reached its
    // normal terminal completion. The agent persists on its own
    // `agents/*` ref (§2.3–§2.4); nothing merges anywhere (§2.6).
    let bare = dest.join("repo.git");
    let mut cmd = Command::new("git");
    for var in INHERITED_GIT_ENV {
        cmd.env_remove(var);
    }
    let out = cmd
        .arg("-C")
        .arg(&bare)
        .args([
            "for-each-ref",
            "--format=%(refname:short)",
            "refs/heads/agents/",
        ])
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&out.stdout).contains(&conv_id),
        "conv branch persists on its agents/* ref (§2.3)"
    );
}
