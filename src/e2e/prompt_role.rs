//! End-to-end proof of the role a root is **born on** through the real
//! binary (ARCH §2.3 `litany prompt --role`, §4.3, §6 *Plan mode is a
//! lineage*, bl-946c).
//!
//! A root used to resolve `worker` and nothing configurable moved it, so
//! putting a whole conversation in plan mode cost a `litany config` pass
//! per workspace. `--role planner` is that pass dissolved: one start,
//! the same config commit, and the confinement is the role's own grant.
//! The subject here is what the *config commit* gave the agent — its
//! soul, its declared tools, and the fact its dispatch commit records —
//! never the reply, which the mock provider fixes.

use super::prompt_fork_point::{Fixture, fixture, git, system_slot};
use std::fs;

/// Declare the shipped `planner` row in the fixture's config lineage,
/// bound to the mock provider (§4.3). The fixture's own amendment
/// rewrites `providers.yaml` to two roles, so the row the template ships
/// has to be re-declared here; the **grant** is the template's verbatim,
/// because it is what the beat is about. `souls/planner.md` is already
/// in the tree — `litany new` seeds it — so nothing re-authors it.
fn declare_planner(fx: &Fixture) {
    let yaml = "\
roles:
  worker:
    provider: test
    model: claude-sonnet-5
    tools: [bash, read_file]
  compactor:
    provider: test
    model: claude-haiku-4-5
  planner:
    provider: test
    model: claude-sonnet-5
    tools: [load_skill, read_file, search_history]
";
    crate::template::authoring::author(
        &fx.ws,
        &fx.ws.join(".no-pools"),
        "default",
        crate::template::authoring::Origin::Advance,
        |dir| fs::write(dir.join("providers.yaml"), yaml),
        &crate::template::RealGit::new(),
    )
    .unwrap();
}

/// The tool names step 1 declared on the wire (§4.3 — the grant *is* the
/// declared toolset), sorted.
fn declared_tools(fx: &Fixture, agent: &str) -> Vec<String> {
    let path = fx.ws.join("steps").join(agent).join("001/request.json");
    let request: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    let mut names: Vec<String> = request["tools"]
        .as_array()
        .expect("a request declares its tools")
        .iter()
        .map(|t| t["name"].as_str().expect("a tool has a name").to_string())
        .collect();
    names.sort();
    names
}

/// The subject of the dispatch commit founding `agent` — the role's one
/// home (`crate::prompt::role`).
fn dispatch_subject(fx: &Fixture, agent: &str) -> String {
    let bare = fx.bare();
    let pattern = crate::prompt::role::founding_pattern(agent);
    git(
        &bare,
        &[
            "log",
            "-n",
            "1",
            "--format=%s",
            "-E",
            "--grep",
            &pattern,
            &format!("agents/{agent}"),
        ],
    )
}

#[test]
fn a_root_born_on_planner_carries_the_planner_grant_and_soul() {
    let fx = fixture();
    declare_planner(&fx);
    let planner = fx.start("how would you add X to Y?", &["--role", "planner"]);

    // The soul is the planner's, pinned onto the dispatch commit from
    // the same config commit the fork chose (§2.3 step 2) and composed
    // into the system slot (§2.8) — not a `souls/worker.md` a config
    // pass had to overwrite.
    let soul = git(&fx.bare(), &["show", &format!("agents/{planner}:soul.md")]);
    assert!(soul.starts_with("# Planner"), "got {soul:?}");
    assert!(
        system_slot(&fx, &planner).ends_with(&soul),
        "the soul closes the system slot"
    );

    // The grant is the confinement (§4.3): the planner row's three
    // tools, and none of the worker's writing ones.
    assert_eq!(
        declared_tools(&fx, &planner),
        ["load_skill", "read_file", "search_history"]
    );

    // And the fact is recorded where every later step reads it (§6
    // role-aware resolution): the dispatch commit's own subject.
    assert_eq!(
        dispatch_subject(&fx, &planner),
        format!("dispatch: planner [{planner}]")
    );
}

#[test]
fn a_start_naming_no_role_is_the_same_path_with_the_worker_default() {
    let fx = fixture();
    let worker = fx.start("do it", &[]);

    let soul = git(&fx.bare(), &["show", &format!("agents/{worker}:soul.md")]);
    assert!(soul.starts_with("# Worker"), "got {soul:?}");
    let tools = declared_tools(&fx, &worker);
    assert!(tools.contains(&"bash".to_string()), "got {tools:?}");
    // The default is written, not omitted: one subject spelling founds
    // every branch, so `--role` is an input to the general path rather
    // than a second shape (bl-946c).
    assert_eq!(
        dispatch_subject(&fx, &worker),
        format!("dispatch: worker [{worker}]")
    );
}

#[test]
fn a_role_the_governing_config_does_not_declare_is_refused_before_any_branch() {
    let fx = fixture();
    let bare = fx.bare();
    let before = git(&bare, &["for-each-ref", "--format=%(refname)"]);

    let out = fx.prompt("hi", &["--role", "ghost"]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(err.contains("ghost"), "{err}");

    // Resolution precedes the fork, so the decline leaves the workspace
    // exactly as it was (§2.3 — every refusal precedes the branch).
    assert_eq!(before, git(&bare, &["for-each-ref", "--format=%(refname)"]));
    assert!(!fx.ws.join("agents").exists());
}
