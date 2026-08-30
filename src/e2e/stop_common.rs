//! Shared helpers for the `litany stop` integration tests
//! (`tests/stop_*.rs`). Lives at `tests/stop_common/mod.rs` so cargo
//! treats it as a module rather than a separate test binary; each
//! consumer pulls it in with `mod stop_common;`.
//!
//! Each integration test crate compiles its own copy of this module
//! and only references a subset of the helpers; `dead_code` is
//! silenced at the module level rather than per-fn.

#![allow(dead_code)]

use std::fs;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};

use super::poll;

pub fn litany_bin() -> std::path::PathBuf {
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

pub fn git_command(dest: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new("git");
    for var in INHERITED_GIT_ENV {
        cmd.env_remove(var);
    }
    cmd.arg("-C").arg(dest).args(args);
    cmd
}

pub fn git_run(dest: &Path, args: &[&str]) {
    let out = git_command(dest, args).output().expect("spawn git");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Global `<harness-root>/models.yaml` (ARCH §4.2) — the shipped shape:
/// no `adapter:` override, no models (bl-35e2). The roles' provider row
/// (`test`) and model ids are the per-repo assignment's alone (§4.3).
pub fn write_global_models(harness: &Path) {
    fs::write(harness.join("models.yaml"), "# no adapter override\n").unwrap();
}

/// A brazen config (§4.4) with a keyless `test` provider row whose
/// endpoint is `endpoint`. Returns the config path.
pub fn write_brazen_config(dir: &Path, endpoint: &str) -> std::path::PathBuf {
    let toml = format!(
        "timeout = 60\n\
         [[provider]]\nname = \"test\"\nbase_url = \"{endpoint}\"\n\
         protocol = \"anthropic_messages\"\nauth = \"none\"\n\
         body_defaults = {{ max_tokens = 64 }}\n"
    );
    let path = dir.join("brazen.toml");
    fs::write(&path, toml).unwrap();
    path
}

/// `<workspace>/repo.git` — the bare workspace repository (ARCH §2.2).
pub fn repo_git(dest: &Path) -> std::path::PathBuf {
    dest.join("repo.git")
}

/// Advance `config/default` with the given control files — the
/// harness-assisted config-commit authoring of ARCH §2.2, over the
/// shipped core (`crate::template::authoring::author`, `Origin::Advance`)
/// rather than hand-rolled worktree juggling. The descriptions refresh
/// reads an absent pool (an empty tree, §3.3), leaving the snapshot
/// `litany new` wrote intact and only landing the given edits.
pub fn amend_config(dest: &Path, files: &[(&str, &str)]) {
    let owned: Vec<(String, String)> = files
        .iter()
        .map(|(r, c)| (r.to_string(), c.to_string()))
        .collect();
    crate::template::authoring::author(
        dest,
        &dest.join(".no-pools"),
        "default",
        crate::template::authoring::Origin::Advance,
        move |dir| {
            for (rel, content) in &owned {
                let path = dir.join(rel);
                fs::create_dir_all(path.parent().unwrap())?;
                fs::write(path, content)?;
            }
            Ok(())
        },
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
    // Point both roles at the fixture `test` brazen row (§4.3) — a
    // config-commit amendment, since control lives in the config
    // lineage (§2.2), never as loose files.
    amend_config(
        dest,
        &[(
            "providers.yaml",
            "\
roles:
  worker:
    provider: test
    model: claude-sonnet-5
  compactor:
    provider: test
    model: claude-haiku-4-5
",
        )],
    );
}

/// Block until an agent branch exists under `agents/*` in the
/// workspace's `repo.git`. The agent id is a bare root conv-id
/// (`<ts>-<short-id>`, 25 chars per §2.3) — the only ref of that shape in
/// a fresh workspace. Panics on early prompt exit (so the caller sees the
/// prompt's stderr) or when the workspace falls silent ([`poll`]).
pub fn poll_for_conv_branch_with_diag(dest: &Path, prompt_child: &mut Child) -> String {
    let repo = repo_git(dest);
    let found = poll::until(dest, || {
        if let Some(name) = scan_conv_branches(&repo) {
            return Some(name);
        }
        if let Ok(Some(status)) = prompt_child.try_wait() {
            let mut buf = Vec::new();
            if let Some(mut e) = prompt_child.stderr.take() {
                use std::io::Read as _;
                let _ = e.read_to_end(&mut buf);
            }
            panic!(
                "litany prompt exited early: {status:?}; stderr: {}",
                String::from_utf8_lossy(&buf)
            );
        }
        None
    });
    if let Some(name) = found {
        return name;
    }
    let buf = drain_stderr(prompt_child);
    let branches = git_command(&repo, &["for-each-ref"])
        .output()
        .expect("spawn git");
    let _ = prompt_child.kill();
    let _ = prompt_child.wait();
    panic!(
        "no agents/* branch under {}, and {} went untouched for {:?}; refs: {:?}; stderr: {}",
        repo.display(),
        dest.display(),
        poll::patience(),
        String::from_utf8_lossy(&branches.stdout),
        String::from_utf8_lossy(&buf)
    );
}

fn drain_stderr(child: &mut Child) -> Vec<u8> {
    use std::io::Read as _;
    use std::os::fd::AsRawFd;
    let Some(stderr) = child.stderr.as_mut() else {
        return Vec::new();
    };
    let fd = stderr.as_raw_fd();
    let prev_flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    unsafe { libc::fcntl(fd, libc::F_SETFL, prev_flags | libc::O_NONBLOCK) };
    let mut buf = Vec::new();
    let _ = stderr.read_to_end(&mut buf);
    unsafe { libc::fcntl(fd, libc::F_SETFL, prev_flags) };
    buf
}

fn scan_conv_branches(repo: &Path) -> Option<String> {
    let out = git_command(
        repo,
        &[
            "for-each-ref",
            "--format=%(refname:short)",
            "refs/heads/agents/",
        ],
    )
    .output()
    .expect("spawn git for-each-ref");
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    for line in s.lines() {
        let Some(name) = line.trim().strip_prefix("agents/") else {
            continue;
        };
        if name.len() == 25 && name.chars().filter(|c| *c == '-').count() == 1 {
            return Some(name.to_owned());
        }
    }
    None
}

/// Block until `path` exists, `dest` being the workspace whose activity
/// is the liveness signal ([`poll`]).
pub fn poll_for_path(dest: &Path, path: &Path) {
    if poll::until(dest, || path.exists().then_some(())).is_none() {
        panic!(
            "{} never appeared, and {} went untouched for {:?}",
            path.display(),
            dest.display(),
            poll::patience()
        );
    }
}

/// Reap `child`, or — once `dest` has gone still around it ([`poll`]) —
/// kill it and report the stall. A stop that failed to reach its target
/// leaves the harness sleeping, which is what this catches; a stop that
/// reached a *slow* machine is not that, so the bound is silence.
pub fn reap(dest: &Path, child: &mut Child) -> ExitStatus {
    let reaped = poll::until(dest, || child.try_wait().expect("try_wait"));
    if let Some(status) = reaped {
        return status;
    }
    let _ = child.kill();
    let _ = child.wait();
    panic!(
        "litany prompt outlived the stop, with {} untouched for {:?}",
        dest.display(),
        poll::patience()
    );
}

pub fn spawn_prompt(
    dest: &Path,
    harness: &Path,
    brazen_config: &Path,
    user_message: &str,
) -> Child {
    Command::new(litany_bin())
        .arg("prompt")
        .arg(dest)
        .arg(user_message)
        .env("LITANY_HOME", harness)
        .env("BRAZEN_CONFIG", brazen_config)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn litany prompt")
}

/// Anthropic-native SSE that resolves to one happy `message_stop`.
/// Used as the eventual-completion body for delayed-mock scenarios.
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
