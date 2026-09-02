//! [`preflight`] — everything the verb refuses before the mark is written
//! (§3.4), so a declined retarget leaves nothing behind.

use super::*;

#[test]
fn a_target_that_does_not_yet_govern_resolves_to_its_commit() {
    // Under follow-the-tip (§2.2, bl-403b) a same-lineage advance is
    // already the agent's next resolution, so what still preflights to a
    // commit is a change of lineage.
    let (_h, ws, _wt) = agent();
    let target = variant(&ws, &[("souls/worker.md", "an amended soul\n")]);
    assert_eq!(preflight(&ws, "a", "variant", &g()).unwrap(), Some(target));
}

#[test]
fn a_targets_own_lineage_advance_resolves_to_none_under_follow_the_tip() {
    // The inverted freeze pin: the agent's next step reads this head
    // anyway, so retargeting onto it is a clean no-op (bl-403b).
    let (_h, ws, _wt) = agent();
    fixture::amend_config(&ws, &[("souls/worker.md", "an amended soul\n")]);
    assert_eq!(
        preflight(&ws, "a", DEFAULT_CONFIG_NAME, &g()).unwrap(),
        None
    );
}

#[test]
fn a_target_already_governing_resolves_to_none_and_writes_nothing() {
    let (_h, ws, _wt) = agent();
    assert_eq!(
        preflight(&ws, "a", DEFAULT_CONFIG_NAME, &g()).unwrap(),
        None
    );
}

#[test]
fn a_path_that_is_not_a_workspace_is_declined() {
    let holder = TempDir::new().unwrap();
    let err = preflight(holder.path(), "a", DEFAULT_CONFIG_NAME, &g()).unwrap_err();
    assert!(err.to_string().contains("is not a workspace"), "{err}");
}

#[test]
fn an_agent_with_no_ref_is_declined_by_the_shared_guard() {
    let (_h, ws) = fixture::workspace();
    let err = preflight(&ws, "nobody", DEFAULT_CONFIG_NAME, &g()).unwrap_err();
    assert!(err.to_string().contains("no agent \"nobody\""), "{err}");
}

#[test]
fn a_config_lineage_the_workspace_lacks_is_declined_naming_the_pool() {
    let (_h, ws, _wt) = agent();
    let err = preflight(&ws, "a", "nosuch", &g()).unwrap_err();
    assert!(err.to_string().contains("no config lineage"), "{err}");
    assert!(err.to_string().contains("default"), "{err}");
}

#[test]
fn a_grant_the_target_does_not_describe_is_declined_before_the_mark() {
    // §3.3 validity-before-fork: `providers.yaml` and `descriptions/**`
    // disagree inside the target commit, so the retarget is refused
    // there rather than forking a branch whose tree cannot be cut.
    let (_h, ws, _wt) = agent();
    variant(
        &ws,
        &[(
            "providers.yaml",
            "roles:\n  worker:\n    provider: anthropic\n    model: claude-sonnet-5\n    \
             tools: [no_such_tool]\n",
        )],
    );
    let err = preflight(&ws, "a", "variant", &g()).unwrap_err();
    assert!(err.to_string().contains("no_such_tool"), "{err}");
    assert!(err.to_string().contains("does not describe"), "{err}");
}

#[test]
fn a_target_whose_providers_yaml_is_malformed_is_declined() {
    let (_h, ws, _wt) = agent();
    variant(&ws, &[("providers.yaml", "roles: [not, a, map]\n")]);
    assert!(preflight(&ws, "a", "variant", &g()).is_err());
}
