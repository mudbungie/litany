//! The model mark at resolution (ARCH §4.3 *The model mark*): the
//! overlay, what it leaves alone, and the fact walking the whole way to
//! the borrowed `Resolved` the step loop reads.

use super::super::tests::Fx;
use super::super::{ConfigSource, resolve_worker};
use super::overlay;
use crate::config::Effort;
use crate::config::per_repo_providers::RoleAssignment;
use crate::template::{GitRunner, RealGit};
use crate::workspace::fixture;
use crate::workspace::model_mark::{self, ModelMark};

const AGENT: &str = "20260101-r1";

fn pick() -> ModelMark {
    ModelMark {
        provider: "openai".into(),
        model: "gpt-5".into(),
        effort: Some(Effort::Low),
        priority: None,
    }
}

#[test]
fn an_unmarked_agent_runs_its_roles_own_binding() {
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, AGENT);
    let fx = Fx::new();
    let cfg = resolve_worker(&ws, ConfigSource::Agent(AGENT), &fx.deps()).unwrap();
    assert_eq!(cfg.provider_row, "anthropic", "the shipped template's row");
    assert_eq!(cfg.model_mark, None);
    assert!(cfg.as_resolved().model_mark.is_none());
}

#[test]
fn a_standing_mark_overlays_the_binding_at_the_next_resolution() {
    // Writing the ref IS the switch: no landing, no re-fork, the next
    // hop reads it.
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, AGENT);
    let fx = Fx::new();
    model_mark::write(&ws, AGENT, &pick(), &RealGit::new()).unwrap();
    let cfg = resolve_worker(&ws, ConfigSource::Agent(AGENT), &fx.deps()).unwrap();
    assert_eq!(cfg.provider_row, "openai");
    assert_eq!(cfg.model_id, "gpt-5");
    assert_eq!(cfg.effort, Some(Effort::Low));
    assert_eq!(cfg.priority, None);
    assert_eq!(cfg.model_mark, Some(pick()));
    let resolved = cfg.as_resolved();
    assert_eq!(resolved.provider_row, "openai");
    assert_eq!(resolved.model_id, "gpt-5");
    assert_eq!(resolved.model_mark, Some(&pick()));
}

#[test]
fn the_mark_leaves_every_other_field_following_the_lineage() {
    // The narrowest pin (DESIGN_CONFIG_FOLLOW §4): a config edit to the
    // grant or the output cap still reaches the marked agent.
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, AGENT);
    let fx = Fx::new();
    model_mark::write(&ws, AGENT, &pick(), &RealGit::new()).unwrap();
    fixture::amend_config(
        &ws,
        &[
            (
                "providers.yaml",
                "roles:\n  worker:\n    provider: anthropic\n    model: claude-sonnet-5\n    \
                 tools: [bash]\n    max_output_tokens: 777\n  compactor:\n    provider: \
                 anthropic\n    model: claude-haiku-4-5\n",
            ),
            ("workflow.yaml", &fixture::workflow_without(&["titler"])),
        ],
    );
    let cfg = resolve_worker(&ws, ConfigSource::Agent(AGENT), &fx.deps()).unwrap();
    assert_eq!(cfg.tools, vec!["bash".to_string()], "the grant followed");
    assert_eq!(cfg.max_output_tokens, Some(777), "the cap followed");
    assert_eq!(cfg.provider_row, "openai", "the mark still binds the model");
}

#[test]
fn clearing_the_mark_returns_the_agent_to_the_lineages_binding() {
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, AGENT);
    let fx = Fx::new();
    let git = RealGit::new();
    model_mark::write(&ws, AGENT, &pick(), &git).unwrap();
    model_mark::clear(&ws, AGENT, &git).unwrap();
    let cfg = resolve_worker(&ws, ConfigSource::Agent(AGENT), &fx.deps()).unwrap();
    assert_eq!(cfg.provider_row, "anthropic");
    assert_eq!(cfg.model_mark, None);
}

#[test]
fn a_fresh_root_about_to_fork_has_no_mark_to_read() {
    // `ConfigSource::Fork` carries no id — the general path with empty
    // inputs, not a case.
    let (_h, ws) = fixture::workspace();
    let fx = Fx::new();
    let cfg = resolve_worker(
        &ws,
        ConfigSource::Fork {
            point: "refs/heads/config/default",
            role: "worker",
        },
        &fx.deps(),
    )
    .unwrap();
    assert_eq!(cfg.model_mark, None);
}

#[test]
fn a_mark_cannot_stand_in_for_a_role_the_config_does_not_declare() {
    // The overlay needs an assignment to overlay; a pick never makes an
    // undeclared role dispatchable (§4.3).
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, AGENT);
    let fx = Fx::new();
    model_mark::write(&ws, AGENT, &pick(), &RealGit::new()).unwrap();
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
    let err = match resolve_worker(&ws, ConfigSource::Agent(AGENT), &fx.deps()) {
        Err(err) => err,
        Ok(_) => panic!("an undeclared role must decline, mark or no mark"),
    };
    assert!(
        matches!(err, crate::prompt::Error::RoleMissing(ref r) if r == "worker"),
        "{err}"
    );
}

#[test]
fn an_unreadable_mark_declines_the_resolution_loudly() {
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, AGENT);
    let fx = Fx::new();
    let git = RealGit::new();
    let repo = crate::workspace::repo_git(&ws);
    let staged = repo.join("junk.tmp");
    std::fs::write(&staged, "provider: [not, a, string]\n").unwrap();
    let blob = git
        .run_capture(
            &repo,
            &["hash-object", "-w", "--", &staged.to_string_lossy()],
        )
        .unwrap();
    git.run(
        &repo,
        &["update-ref", &model_mark::model_ref(AGENT), blob.trim()],
    )
    .unwrap();
    let err = match resolve_worker(&ws, ConfigSource::Agent(AGENT), &fx.deps()) {
        Err(err) => err,
        Ok(_) => panic!("a mark that does not parse must decline, not fall back"),
    };
    assert!(matches!(err, crate::prompt::Error::Config(_)), "{err}");
    assert!(err.to_string().contains("refs/litany/model/"), "{err}");
}

#[test]
fn the_overlay_replaces_the_knobs_whole_and_keeps_the_rest() {
    let mut assignment = RoleAssignment {
        provider: "anthropic".into(),
        model: "claude-sonnet-5".into(),
        tools: vec!["bash".into()],
        effort: Some(Effort::High),
        priority: Some(true),
        max_output_tokens: Some(9000),
    };
    overlay(
        &mut assignment,
        &ModelMark {
            provider: "openai".into(),
            model: "gpt-5".into(),
            effort: None,
            priority: None,
        },
    );
    assert_eq!(assignment.provider, "openai");
    assert_eq!(assignment.model, "gpt-5");
    // Absent on the mark means none requested — not "keep the lineage's".
    assert_eq!(assignment.effort, None);
    assert_eq!(assignment.priority, None);
    assert_eq!(assignment.tools, vec!["bash".to_string()]);
    assert_eq!(assignment.max_output_tokens, Some(9000));
}
