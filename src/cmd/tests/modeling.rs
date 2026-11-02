//! `model` (ARCH §4.3 *The model mark*, §3.4): the pre-flights, the
//! mark, the clear and the read — the switch itself lands at resolution
//! (`prompt::resolve::model_source`), which is pinned there.

use super::{noop_editor, with_fx};
use crate::cmd::{Outcome, model};
use crate::config::Effort;
use crate::template::RealGit;
use crate::workspace::model_mark::{self, ModelMark};
use crate::workspace::{self, fixture};

const AGENT: &str = "20260101-a1";

/// Drive the verb, returning its `Outcome`. A write's confirmation is
/// an `eprintln!` on the process's own stderr (§3.4), so what a test
/// reads back after one is the mark itself; a read's whole answer is
/// the `Outcome::Line` product.
fn call(
    ws: &std::path::Path,
    agent: &str,
    pick: Option<(&str, &str)>,
    effort: Option<Effort>,
    priority: bool,
    clear: bool,
) -> Result<Outcome, crate::cmd::Error> {
    let (r, out, _err) = with_fx("true", b"", &noop_editor, |fx| {
        model::run(
            model::Args {
                workspace: ws.to_path_buf(),
                agent: agent.to_string(),
                provider: pick.map(|(p, _)| p.to_string()),
                model: pick.map(|(_, m)| m.to_string()),
                effort,
                priority,
                clear,
            },
            fx,
        )
    });
    assert!(out.is_empty(), "the product is the Outcome, not a print");
    r
}

/// A write: product-less on stdout (§3.4).
fn pick(ws: &std::path::Path, provider: &str, model: &str) {
    assert!(
        matches!(
            call(ws, AGENT, Some((provider, model)), None, false, false).unwrap(),
            Outcome::Quiet
        ),
        "a write is product-less (§3.4)",
    );
}

/// The read: the one line the bare invocation answers with.
fn read(ws: &std::path::Path) -> String {
    match call(ws, AGENT, None, None, false, false).unwrap() {
        Outcome::Line(line) => line,
        other => panic!("the read answers one line, got {other:?}"),
    }
}

#[test]
fn a_pick_writes_the_standing_mark_with_the_whole_binding() {
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, AGENT);
    let r = call(
        &ws,
        AGENT,
        Some(("openai", "gpt-5")),
        Some(Effort::High),
        true,
        false,
    );
    assert!(matches!(r.unwrap(), Outcome::Quiet));
    assert_eq!(
        model_mark::read(&ws, AGENT, &RealGit::new()).unwrap(),
        Some(ModelMark {
            provider: "openai".into(),
            model: "gpt-5".into(),
            effort: Some(Effort::High),
            priority: Some(true),
        }),
        "the mark is the verb's whole effect",
    );
}

#[test]
fn an_unasked_priority_is_absent_on_the_mark_not_false() {
    // `false` and omitted are one fact on the assignment (§4.3), and the
    // mark spells it the assignment's way.
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, AGENT);
    pick(&ws, "openai", "gpt-5");
    let mark = model_mark::read(&ws, AGENT, &RealGit::new())
        .unwrap()
        .unwrap();
    assert_eq!(mark.priority, None);
    assert_eq!(mark.effort, None);
}

#[test]
fn bare_reads_the_lineages_binding_and_writes_nothing() {
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, AGENT);
    let line = read(&ws);
    assert_eq!(
        line,
        format!(
            "{AGENT} runs anthropic / claude-sonnet-5 as worker — followed from its governing lineage"
        ),
    );
    assert_eq!(
        model_mark::read(&ws, AGENT, &RealGit::new()).unwrap(),
        None,
        "a read writes no ref",
    );
}

#[test]
fn the_read_names_the_mark_and_its_knobs() {
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, AGENT);
    call(
        &ws,
        AGENT,
        Some(("openai", "gpt-5")),
        Some(Effort::Low),
        true,
        false,
    )
    .unwrap();
    let line = read(&ws);
    assert_eq!(
        line,
        format!(
            "{AGENT} runs openai / gpt-5 (effort low, priority) as worker — marked on [{AGENT}]"
        ),
    );
}

#[test]
fn the_read_shows_the_lineages_own_knobs_when_unmarked() {
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, AGENT);
    fixture::amend_config(
        &ws,
        &[
            (
                "providers.yaml",
                "roles:\n  worker:\n    provider: anthropic\n    model: claude-sonnet-5\n    \
                 effort: medium\n  compactor:\n    provider: anthropic\n    model: \
                 claude-haiku-4-5\n",
            ),
            ("workflow.yaml", &fixture::workflow_without(&["titler"])),
        ],
    );
    let line = read(&ws);
    assert!(line.contains("(effort medium)"), "{line}");
    assert!(
        line.contains("followed from its governing lineage"),
        "{line}"
    );
}

#[test]
fn clear_deletes_the_mark_and_deletes_config_not_code() {
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, AGENT);
    pick(&ws, "openai", "gpt-5");
    assert!(matches!(
        call(&ws, AGENT, None, None, false, true).unwrap(),
        Outcome::Quiet
    ));
    assert_eq!(model_mark::read(&ws, AGENT, &RealGit::new()).unwrap(), None);
    assert!(
        read(&ws).contains("followed from its governing lineage"),
        "and the read says so",
    );
}

#[test]
fn a_second_pick_replaces_the_first() {
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, AGENT);
    pick(&ws, "openai", "gpt-5");
    pick(&ws, "anthropic", "claude-opus-4-1");
    let mark = model_mark::read(&ws, AGENT, &RealGit::new())
        .unwrap()
        .unwrap();
    assert_eq!(
        (mark.provider.as_str(), mark.model.as_str()),
        ("anthropic", "claude-opus-4-1")
    );
}

#[test]
fn an_unknown_agent_is_refused_before_any_mark() {
    let (_h, ws) = fixture::workspace();
    let err = call(
        &ws,
        "20260101-zz",
        Some(("openai", "gpt-5")),
        None,
        false,
        false,
    )
    .unwrap_err();
    let s = err.to_string();
    assert!(s.starts_with("litany model:"), "{s}");
    assert!(s.contains("20260101-zz"), "{s}");
    assert_eq!(
        model_mark::read(&ws, "20260101-zz", &RealGit::new()).unwrap(),
        None,
        "a declined pick leaves no debris",
    );
}

#[test]
fn a_role_the_followed_config_does_not_declare_is_refused_before_any_mark() {
    // A mark overlays a binding; it cannot stand in for one (§4.3).
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, AGENT);
    fixture::amend_config(
        &ws,
        &[
            (
                "providers.yaml",
                "roles:\n  compactor:\n    provider: anthropic\n    model: claude-haiku-4-5\n",
            ),
            (
                "workflow.yaml",
                &fixture::workflow_without(&["titler", "worker"]),
            ),
        ],
    );
    let err = call(&ws, AGENT, Some(("openai", "gpt-5")), None, false, false).unwrap_err();
    let s = err.to_string();
    assert!(s.contains("declares no \"worker\" role"), "{s}");
    assert_eq!(model_mark::read(&ws, AGENT, &RealGit::new()).unwrap(), None);
}

#[test]
fn a_hand_written_mark_that_does_not_parse_fails_the_read_loudly() {
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, AGENT);
    let git = RealGit::new();
    let repo = workspace::repo_git(&ws);
    let staged = repo.join("junk.tmp");
    std::fs::write(&staged, "nonsense: 1\n").unwrap();
    let blob = crate::template::GitRunner::run_capture(
        &git,
        &repo,
        &["hash-object", "-w", "--", &staged.to_string_lossy()],
    )
    .unwrap();
    crate::template::GitRunner::run(
        &git,
        &repo,
        &["update-ref", &model_mark::model_ref(AGENT), blob.trim()],
    )
    .unwrap();
    let err = call(&ws, AGENT, None, None, false, false).unwrap_err();
    assert!(err.to_string().contains("refs/litany/model/"), "{err}");
}
