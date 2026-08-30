//! Coverage for the agent / bundler seams (ARCH §9.3, §9.2).
//!
//! `CommandAgent` and `CommandBundler` are exercised against shell stubs
//! standing in for the harness driver and `litany bundle` — no live model
//! traffic, exactly as the runner's testability requires (§9.3).

use agent_eval::agent::{Agent, BundleTarget, Bundler, CommandAgent, CommandBundler, Dispatch};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

/// Serializes script-write-then-spawn pairs across tests. Without this, a
/// concurrent posix_spawn in another thread inherits the write fd held by
/// fs::write in this thread; that fd is CLOEXEC but only closes once the
/// peer's own exec completes. If this thread's exec on the script it just
/// wrote lands while the peer child still holds the inherited write fd,
/// Linux returns ETXTBSY. Holding one lock across write + spawn in every
/// test eliminates the overlap window — it must be a single static for the
/// whole binary: per-module locks do not exclude each other's threads.
static SPAWN_LOCK: Mutex<()> = Mutex::new(());

fn spawn_lock() -> MutexGuard<'static, ()> {
    SPAWN_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Write an executable `sh` stub and return its path.
fn stub(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    let mut perm = std::fs::metadata(&path).unwrap().permissions();
    perm.set_mode(0o755);
    std::fs::set_permissions(&path, perm).unwrap();
    path
}

/// Invoke a stub agent whose body writes `report_body` to the report file,
/// and return the parsed outcome target.
fn dispatch_with(report_body: &str) -> Option<BundleTarget> {
    let _g = spawn_lock();
    let d = tempfile::tempdir().unwrap();
    let body = match report_body {
        "<none>" => "exit 0".to_string(),
        "<empty>" => ": > \"$LITANY_EVAL_REPORT\"".to_string(),
        other => format!("printf '{other}' > \"$LITANY_EVAL_REPORT\""),
    };
    let prog = stub(d.path(), "agent.sh", &body);
    let home = d.path().join("home");
    let work = d.path().join("work");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&work).unwrap();
    let agent = CommandAgent::new(prog);
    agent
        .dispatch(&Dispatch {
            prompt: "do the thing",
            workdir: &work,
            litany_home: &home,
            experiment: Path::new("/x/workflow.yaml"),
        })
        .unwrap()
        .target
}

#[test]
fn report_variants_parse() {
    // Good two-line report -> Some.
    let t = dispatch_with("ws\\nid\\n").unwrap();
    assert_eq!(t.workspace, PathBuf::from("ws"));
    assert_eq!(t.agent_id, "id");
    // No report file at all -> None (read error arm).
    assert_eq!(dispatch_with("<none>"), None);
    // Empty file -> None (missing workspace line).
    assert_eq!(dispatch_with("<empty>"), None);
    // One line only -> None (missing agent line).
    assert_eq!(dispatch_with("ws\\n"), None);
    // Empty workspace field -> None (first operand of the guard).
    assert_eq!(dispatch_with("\\nid\\n"), None);
    // Empty agent field -> None (second operand of the guard).
    assert_eq!(dispatch_with("ws\\n\\n"), None);
}

#[test]
fn agent_exit_code_is_ignored() {
    // A stub that exits non-zero but writes a valid report: the exit code
    // is not the pass signal (§9.1), so the target still parses.
    let _g = spawn_lock();
    let d = tempfile::tempdir().unwrap();
    let prog = stub(
        d.path(),
        "agent.sh",
        "printf 'ws\\nid\\n' > \"$LITANY_EVAL_REPORT\"; exit 3",
    );
    let home = d.path().join("home");
    let work = d.path().join("work");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&work).unwrap();
    let agent = CommandAgent::new(prog);
    let out = agent
        .dispatch(&Dispatch {
            prompt: "p",
            workdir: &work,
            litany_home: &home,
            experiment: Path::new("/x"),
        })
        .unwrap();
    assert!(out.target.is_some());
}

#[test]
fn agent_spawn_failure_is_an_error() {
    // Writes no stub, but its own spawn still forks: an unlocked fork here
    // inherits a peer's write fd and gives that peer the ETXTBSY.
    let _g = spawn_lock();
    let agent = CommandAgent::new("agent-eval-no-such-binary-xyz");
    let d = tempfile::tempdir().unwrap();
    let err = agent
        .dispatch(&Dispatch {
            prompt: "p",
            workdir: d.path(),
            litany_home: d.path(),
            experiment: Path::new("/x"),
        })
        .unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    // Actionable: the message names the program that failed to spawn.
    let msg = err.to_string();
    assert!(msg.contains("--agent agent-eval-no-such-binary-xyz"));
}

#[test]
fn bundler_success_and_failure() {
    let _g = spawn_lock();
    let d = tempfile::tempdir().unwrap();
    let dest = d.path().join("out");
    #[rustfmt::skip]
    let target = BundleTarget { workspace: d.path().join("ws"), agent_id: "a1".to_string() };

    // A stub that records its argv and exits 0.
    let ok = stub(
        d.path(),
        "ok.sh",
        "printf '%s\\n' \"$@\" > \"$(dirname \"$0\")/argv\"; exit 0",
    );
    CommandBundler::new(&ok)
        .bundle(&target, &dest)
        .expect("bundle ok");
    let argv = std::fs::read_to_string(d.path().join("argv")).unwrap();
    // litany bundle <workspace> <agent> <dest>
    assert!(argv.contains("bundle"));
    assert!(argv.contains("a1"));

    // A stub that fails -> Err.
    let bad = stub(d.path(), "bad.sh", "exit 1");
    let err = CommandBundler::new(&bad)
        .bundle(&target, &dest)
        .unwrap_err();
    assert!(err.to_string().contains("litany bundle exited"));
}
