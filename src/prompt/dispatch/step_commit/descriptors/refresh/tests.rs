//! The boundary re-cut, against a real repository (ARCH §3.3 × §2.2,
//! bl-37cd) — because what is under test is whether git's own answer to
//! *did anything move* lands the right commit, and a stub git answers
//! that question by fiat.

use super::*;
use crate::template::RealGit;
use std::io;

/// A repository whose HEAD is the config commit: it carries schemas for
/// `bash` and `message` plus `message`'s claimed frontmatter, and its
/// worktree is cut to `keep`, committed. That is the state a dispatch
/// commit leaves behind, and the state a boundary re-cut inherits.
fn repo(keep: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    let (wt, g) = (dir.path(), RealGit::new());
    g.run(wt, &["init", "-b", "agents/a1"]).unwrap();
    g.run(wt, &["config", "user.email", "t@t"]).unwrap();
    g.run(wt, &["config", "user.name", "t"]).unwrap();
    g.run(wt, &["config", "core.hooksPath", "/dev/null"])
        .unwrap();
    std::fs::create_dir_all(wt.join("descriptions/tools")).unwrap();
    std::fs::create_dir_all(wt.join("descriptions/skills")).unwrap();
    for tool in ["bash", "message"] {
        std::fs::write(
            wt.join(format!("descriptions/tools/{tool}.json")),
            format!("{{\"schema\":\"{tool}\"}}\n"),
        )
        .unwrap();
    }
    std::fs::write(wt.join("descriptions/skills/message.md"), "how to send\n").unwrap();
    g.run(wt, &["add", "-A"]).unwrap();
    g.run(wt, &["commit", "-m", "config"]).unwrap();
    // The fork's own cut, as a commit: everything outside `keep` goes.
    let drop: Vec<String> = ["bash", "message"]
        .iter()
        .filter(|t| !keep.contains(t))
        .flat_map(|t| {
            [
                format!("descriptions/tools/{t}.json"),
                format!("descriptions/skills/{t}.md"),
            ]
        })
        .collect();
    if !drop.is_empty() {
        let mut args: Vec<&str> = vec!["rm", "-q", "--ignore-unmatch", "--"];
        args.extend(drop.iter().map(String::as_str));
        g.run(wt, &args).unwrap();
        g.run(wt, &["commit", "-m", "fork: cut to the grant"])
            .unwrap();
    }
    dir
}

/// The config commit — HEAD's parent chain root here, and the only
/// commit carrying the whole snapshot.
fn config_commit(wt: &std::path::Path) -> String {
    RealGit::new()
        .run_capture(wt, &["rev-list", "--max-parents=0", "HEAD"])
        .unwrap()
        .trim()
        .to_string()
}

fn head(wt: &std::path::Path) -> String {
    RealGit::new()
        .run_capture(wt, &["rev-parse", "HEAD"])
        .unwrap()
        .trim()
        .to_string()
}

fn run(wt: &std::path::Path, tools: &[&str]) -> Result<bool, Error> {
    let owned: Vec<String> = tools.iter().map(|t| (*t).to_string()).collect();
    refresh(
        wt,
        "a1",
        &Grant {
            role: "worker",
            tools: &owned,
            config_commit: &config_commit(wt),
        },
        &RealGit::new(),
    )
}

#[test]
fn a_tip_that_widens_the_grant_brings_the_new_descriptors_into_the_tree() {
    // The half a request cannot work around: the role now grants
    // `message`, the wire declares it, and nothing in the agent's tree
    // described it — so the model was told to call an instrument it had
    // no documentation for.
    let dir = repo(&["bash"]);
    let wt = dir.path();
    let before = head(wt);
    assert!(!wt.join("descriptions/tools/message.json").exists());

    assert!(run(wt, &["bash", "message"]).unwrap(), "it committed");
    assert!(wt.join("descriptions/tools/message.json").exists());
    assert!(
        wt.join("descriptions/skills/message.md").exists(),
        "the claimed frontmatter comes with its schema (§3.3)"
    );
    assert_ne!(head(wt), before, "and it landed as a commit");
    let subject = RealGit::new()
        .run_capture(wt, &["log", "-1", "--format=%s"])
        .unwrap();
    assert!(subject.contains("descriptors: follow the config tip [a1]"));
}

#[test]
fn a_tip_that_revokes_a_grant_takes_the_stale_descriptors_out_of_the_tree() {
    // The other half, and the one the cut exists for: after a revoke,
    // `descriptions/tools/` must still BE the callable set — a schema
    // the wire no longer declares is exactly the convincing on-disk
    // documentation the cut was built to stop (yog bl-55b1).
    let dir = repo(&["bash", "message"]);
    let wt = dir.path();
    assert!(run(wt, &["bash"]).unwrap(), "it committed");
    assert!(!wt.join("descriptions/tools/message.json").exists());
    assert!(!wt.join("descriptions/skills/message.md").exists());
    assert!(wt.join("descriptions/tools/bash.json").exists());
}

#[test]
fn an_unchanged_boundary_commits_nothing() {
    // The general path — every boundary at which the followed commit
    // did not move. git answers the has-anything-changed question, so
    // no record of the last cut is kept anywhere.
    let dir = repo(&["bash"]);
    let wt = dir.path();
    let before = head(wt);
    assert!(!run(wt, &["bash"]).unwrap(), "nothing moved");
    assert_eq!(head(wt), before, "so no commit was made");
}

#[test]
fn a_granted_tool_the_commit_does_not_describe_is_noticed_never_declined() {
    // At the fork this is a refusal (`require_described`). At a
    // boundary it must not be: killing a running conversation over an
    // operator's config edit is the failure class follow-the-tip exists
    // to fix (§2.2). The tool is simply not in the tree, which is
    // already how `tools::compose` reads an absent schema.
    let dir = repo(&["bash"]);
    let wt = dir.path();
    let before = head(wt);
    assert!(!run(wt, &["bash", "ghost"]).unwrap());
    assert_eq!(head(wt), before);
    assert!(
        wt.join("descriptions/tools/bash.json").exists(),
        "and the described half of the grant is untouched"
    );
}

#[test]
fn a_still_granted_tool_the_commit_stopped_describing_keeps_its_bytes() {
    // Undescribed is not revoked. The grant still names it, so the
    // whole-grant drop leaves it alone: deleting the only surviving
    // description on the strength of a config disagreement would
    // destroy information to enforce a rule about documentation.
    let dir = repo(&["bash", "message"]);
    let wt = dir.path();
    let g = RealGit::new();
    // A newer config commit that describes `bash` alone.
    g.run(wt, &["rm", "-q", "--", "descriptions/tools/message.json"])
        .unwrap();
    g.run(wt, &["commit", "-m", "config: stop describing message"])
        .unwrap();
    let narrowed = head(wt);
    std::fs::write(
        wt.join("descriptions/tools/message.json"),
        "{\"schema\":\"message\"}\n",
    )
    .unwrap();
    g.run(wt, &["add", "-A"]).unwrap();
    g.run(wt, &["commit", "-m", "the tree still carries it"])
        .unwrap();

    let owned = ["bash".to_string(), "message".to_string()];
    let moved = refresh(
        wt,
        "a1",
        &Grant {
            role: "worker",
            tools: &owned,
            config_commit: &narrowed,
        },
        &g,
    )
    .unwrap();
    assert!(!moved);
    assert!(wt.join("descriptions/tools/message.json").exists());
}

/// A runner that answers every git call except the one named.
struct OneFails(&'static str);
impl GitRunner for OneFails {
    fn run(&self, _dir: &std::path::Path, args: &[&str]) -> io::Result<()> {
        match args.first() {
            Some(op) if *op == self.0 => Err(io::Error::other("boom")),
            _ => Ok(()),
        }
    }
    fn run_capture(&self, _dir: &std::path::Path, args: &[&str]) -> io::Result<String> {
        match args.first() {
            Some(op) if *op == self.0 => Err(io::Error::other("boom")),
            // A dirty tree, so the commit arm is reached.
            _ => Ok(" M descriptions/tools/bash.json\n".to_string()),
        }
    }
}

#[test]
fn a_failing_status_and_a_failing_commit_each_surface_as_a_named_git_error() {
    let owned = ["bash".to_string()];
    let grant = Grant {
        role: "worker",
        tools: &owned,
        config_commit: "cafe1234",
    };
    let wt = std::path::Path::new("/nowhere");
    let err = refresh(wt, "a1", &grant, &OneFails("status")).unwrap_err();
    assert!(
        matches!(
            err,
            Error::Git {
                op: "descriptor refresh status",
                ..
            }
        ),
        "{err:?}"
    );
    let err = refresh(wt, "a1", &grant, &OneFails("commit")).unwrap_err();
    assert!(
        matches!(
            err,
            Error::Git {
                op: "descriptor refresh commit",
                ..
            }
        ),
        "{err:?}"
    );
}
