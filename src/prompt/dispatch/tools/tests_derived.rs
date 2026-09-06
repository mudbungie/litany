//! The composer against a tree a *real dispatch* derived (ARCH §3.3).
//!
//! [`super::tests`] exercises `compose` against hand-written worktrees.
//! This one closes the loop the other half of §3.3 opens: what the
//! dispatch commit puts in the child's tree is what the child's request
//! declares. Split into its own file so both stay under the 300-line cap.

use super::tests::granted;
use super::*;
use crate::prompt::child_dispatch::{ChildDispatchRequest, run};
use crate::prompt::inbox::Launcher;
use crate::template::RealGit;
use crate::workspace::{self, fixture};
use std::io;

/// A [`Launcher`] that does nothing: these tests assert on trees, not on
/// the front door (`child_dispatch::tests` covers that).
struct NoLaunch;
impl Launcher for NoLaunch {
    fn launch(&self, _ws: &Path, _agent: &str) -> io::Result<()> {
        Ok(())
    }
}

/// Dispatch `role` off `parent`'s tip, returning the child's id.
fn dispatch_role(ws: &Path, parent: &str, role: &str) -> String {
    let parent_wt = workspace::agent_worktree(ws, parent);
    run(
        &ChildDispatchRequest {
            repo: ws,
            parent_branch: parent,
            parent_worktree: &parent_wt,
            role,
            goal: "g",
            name: None,
            fork_point: None,
            cwd: None,
            pins: crate::prompt::PinnedDocs::none(),
        },
        &RealGit::new(),
        &crate::prompt::SystemClock,
        &crate::prompt::NanoIdGen,
        &NoLaunch,
        crate::workspace::agent_name::mint::test_rng(),
    )
    .unwrap()
}

fn declared(names: &[&str]) -> Vec<String> {
    names.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn a_dispatched_role_declares_its_whole_grant_whatever_its_dispatchers_was() {
    // bl-a900: the `sensor` is dispatched by a `worker` whose grant lacks
    // `message`. Its descriptors are derived from the governing config
    // commit, so the intersection this module composes (declaration ∩
    // availability) no longer silently loses the role's one instrument.
    let (_h, ws) = fixture::workspace();
    fixture::amend_config(
        &ws,
        &[
            ("souls/worker.md", "worker soul\n"),
            ("souls/sensor.md", "sensor soul\n"),
            (
                "providers.yaml",
                "roles:\n  worker:\n    provider: anthropic\n    \
                 model: claude-sonnet-5\n    tools: [bash]\n  sensor:\n    \
                 provider: anthropic\n    model: claude-sonnet-5\n    \
                 tools: [message]\n",
            ),
            ("descriptions/tools/bash.json", "{\"type\":\"object\"}\n"),
            (
                "descriptions/skills/bash.md",
                "name: bash\ndescription: runs a command\n",
            ),
            ("descriptions/tools/message.json", "{\"type\":\"object\"}\n"),
            (
                "descriptions/skills/message.md",
                "name: message\ndescription: sends a message\n",
            ),
        ],
    );
    fixture::spawn_root(&ws, "20260101-pc");
    let worker = dispatch_role(&ws, "20260101-pc", "worker");
    let sensor = dispatch_role(&ws, &worker, "sensor");

    let tools = compose(
        &workspace::agent_worktree(&ws, &sensor),
        &granted(&declared(&["message"])),
        &[],
        &[],
    )
    .unwrap();
    let names: Vec<&str> = tools.iter().map(tool_name).collect();
    assert_eq!(names, vec!["message"], "the grant rides the wire whole");
    // The description half rides with it (§3.3 point 3): the entry is
    // populated, not a bare name.
    match &tools[0] {
        Tool::Custom { description, .. } => {
            assert_eq!(description.as_deref(), Some("sends a message"));
        }
        other => panic!("got {other:?}"),
    }

    // The dispatcher's own request is unchanged by any of this.
    let worker_tools = compose(
        &workspace::agent_worktree(&ws, &worker),
        &granted(&declared(&["bash"])),
        &[],
        &[],
    )
    .unwrap();
    assert_eq!(
        worker_tools.iter().map(tool_name).collect::<Vec<_>>(),
        vec!["bash"]
    );
}
