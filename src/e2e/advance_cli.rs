//! End-to-end test for `litany advance` (ARCH §6): the reprompt chain
//! and the exec baton, over real subprocesses and real `bz`. Flow: `litany prompt` answers "ping" and quiesces. `litany message`
//! deposits a reprompt and — the §2.11 probe finding the lease free —
//! detach-spawns `litany advance`, which delivers the deposit and steps.
//! The model call returns `tool_use`, so the hop runs the bash tool and
//! **exec's its successor** with the lock fd riding `LITANY_LOCK_FD`
//! (§6 exec baton); the successor adopts the lease, finds the tail
//! user-side, and steps to the final response — whose exit protocol
//! launches one last driver that finds nothing due (§2.11 pin 1).
//!
//! The scripted TCP server (the `prompt_retry.rs` pattern) serves one
//! response per connection: ping → tool_use → final. The compactor is
//! the v0.3 stub (no model call), so it opens no connection.

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

use super::poll;

fn litany_bin() -> std::path::PathBuf {
    crate::test_support::litany_binary()
}

const HAPPY_SSE: &str = concat!(
    "event: message_start\n",
    "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_a\",\"model\":\"claude-sonnet-5\",\"stop_reason\":null,\"content\":[],\"usage\":{\"input_tokens\":2,\"output_tokens\":0}}}\n\n",
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

/// A `tool_use` completion: run `echo BATON-OK` through the bash tool.
const TOOL_USE_SSE: &str = concat!(
    "event: message_start\n",
    "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_t\",\"model\":\"claude-sonnet-5\",\"stop_reason\":null,\"content\":[],\"usage\":{\"input_tokens\":2,\"output_tokens\":0}}}\n\n",
    "event: content_block_start\n",
    "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_01\",\"name\":\"bash\",\"input\":{}}}\n\n",
    "event: content_block_delta\n",
    "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"command\\\":\\\"echo BATON-OK\\\"}\"}}\n\n",
    "event: content_block_stop\n",
    "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
    "event: message_delta\n",
    "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":5}}\n\n",
    "event: message_stop\n",
    "data: {\"type\":\"message_stop\"}\n\n",
);

/// Serve one scripted SSE body per incoming connection.
fn spawn_seq_server(bodies: Vec<&'static str>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for body in bodies {
            let (mut stream, _) = listener.accept().expect("accept");
            drain_http_request(&mut stream);
            let resp = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\n\
                 content-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(resp.as_bytes()).expect("write response");
            stream.flush().expect("flush");
        }
    });
    format!("http://127.0.0.1:{port}")
}

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

const ROLES_YAML: &str = "\
roles:
  worker:
    provider: test
    model: claude-sonnet-5
    tools: [bash]
  compactor:
    provider: test
    model: claude-haiku-4-5
";

fn scaffold(dest: &Path, harness: &Path) {
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
    // Config-commit amendment (§2.2): point roles at the fixture row,
    // over the shipped authoring core rather than hand-rolled worktrees.
    crate::template::authoring::author(
        dest,
        &dest.join(".no-pools"),
        "default",
        crate::template::authoring::Origin::Advance,
        |dir| fs::write(dir.join("providers.yaml"), ROLES_YAML),
        &crate::template::RealGit::new(),
    )
    .unwrap();
}

/// Poll for `path` to exist — the driver chain runs detached, so the test
/// observes disk, exactly like a frontend (§3.5). The bound is [`poll`]'s:
/// the chain may take as long as the box makes it take, and only a
/// motionless `workspace` fails.
fn wait_for(workspace: &Path, path: &Path) {
    if poll::until(workspace, || path.exists().then_some(())).is_none() {
        panic!(
            "{path:?} never appeared, and {} went untouched for {:?} — nothing is driving it",
            workspace.display(),
            poll::patience()
        );
    }
}

#[test]
fn message_launches_a_detached_advance_chain_that_batons_through_tools() {
    let endpoint = spawn_seq_server(vec![HAPPY_SSE, TOOL_USE_SSE, HAPPY_SSE]);
    let holder = TempDir::new().unwrap();
    let harness = holder.path().join("harness");
    fs::create_dir_all(&harness).unwrap();
    write_global_models(&harness);
    let brazen_config = write_brazen_config(holder.path(), &endpoint);
    let dest = holder.path().join("conv");
    scaffold(&dest, &harness);

    // Exchange 1: `litany prompt` answers and quiesces. Its exit launch
    // spawns a real driver, which finds nothing due and exits silently
    // (§2.11 pin 1) — the recursion terminator, live.
    let out = Command::new(litany_bin())
        .arg("prompt")
        .arg(&dest)
        .arg("ping")
        .env("LITANY_HOME", &harness)
        .env("BRAZEN_CONFIG", &brazen_config)
        .output()
        .expect("spawn litany prompt");
    assert!(
        out.status.success(),
        "litany prompt: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let conv = String::from_utf8(out.stdout).unwrap().trim().to_string();

    // Exchange 2: the reprompt is a message (§2.4). The deposit probe
    // finds the lease free and detach-spawns `litany advance`; the verb
    // returns immediately — delivery and stepping continue in the driver.
    let out = Command::new(litany_bin())
        .arg("message")
        .arg(&dest)
        .arg(&conv)
        .arg("again")
        .env("LITANY_HOME", &harness)
        .env("BRAZEN_CONFIG", &brazen_config)
        .output()
        .expect("spawn litany message");
    assert!(
        out.status.success(),
        "litany message: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // The chain: deliver (003-user) → step 2 (004 tool_use) → bash tool
    // (005-tool) → exec successor with the lease riding LITANY_LOCK_FD →
    // step 3 (006 final response).
    let messages = dest.join("agents").join(&conv).join("messages");
    wait_for(&dest, &messages.join("003-user.md"));
    wait_for(&dest, &messages.join("004-claude-sonnet-5.json"));
    wait_for(&dest, &messages.join("005-tool.json"));
    wait_for(&dest, &messages.join("006-claude-sonnet-5.json"));

    let tool_entry = fs::read_to_string(messages.join("005-tool.json")).unwrap();
    assert!(tool_entry.contains("BATON-OK"), "got {tool_entry:?}");

    // Both hops recorded their steps at the derived sequence; the
    // successor's response closed with a terminal `end` (§4.4).
    let step3 = dest.join(format!("steps/{conv}/003/response.json"));
    wait_for(&dest, &step3);
    let ended = poll::until(&dest, || {
        let lines: Vec<serde_json::Value> = fs::read(&step3)
            .unwrap()
            .split(|b| *b == b'\n')
            .filter(|l| !l.is_empty())
            .map(|l| serde_json::from_slice(l).expect("valid JSON line"))
            .collect();
        lines
            .last()
            .is_some_and(|e| e["type"] == "end")
            .then_some(())
    });
    assert!(
        ended.is_some(),
        "step 3 never completed, and the workspace went untouched for {:?}",
        poll::patience()
    );
}

#[test]
fn advance_verb_surfaces_an_unusable_workspace_loudly() {
    let out = Command::new(litany_bin())
        .args(["advance", "/no/such/workspace", "20260101-a1"])
        .output()
        .expect("spawn litany advance");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("litany advance"));
}

#[test]
fn advance_on_a_name_with_no_agent_ref_refuses_and_mints_no_inbox() {
    // A real workspace and an id that names no agent: the §2.3
    // existence guard refuses in the `litany message` voice, exit 1 —
    // and, because the guard runs ahead of the lease, `inbox/<name>/`
    // is never created. Before the guard this exited 0 in silence and
    // left the orphan directory behind.
    let holder = TempDir::new().unwrap();
    let harness = holder.path().join("harness");
    fs::create_dir_all(&harness).unwrap();
    write_global_models(&harness);
    let dest = holder.path().join("ws");
    scaffold(&dest, &harness);
    let out = Command::new(litany_bin())
        .args(["advance"])
        .arg(&dest)
        .arg("ghost")
        .env("LITANY_HOME", &harness)
        .output()
        .expect("spawn litany advance");
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty(), "a refusal is stderr-only");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.trim_end().ends_with(
            "litany advance: no agent \"ghost\" in this workspace — a hop drives an existing \
             agent (ARCH §2.3: the `agents/*` refs are the registry); check the id against \
             the workspace's `agents/*` refs, or start an agent with `litany prompt` / \
             `litany dispatch`"
        ),
        "{stderr}"
    );
    assert!(
        !dest.join("inbox").join("ghost").exists(),
        "the refusal mints no inbox directory"
    );
}
