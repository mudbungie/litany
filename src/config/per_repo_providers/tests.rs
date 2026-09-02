//! Parse tests for [`super`] (split out to hold the per-file line cap):
//! the `roles:` shape and its optional per-assignment knobs, the closed
//! legacy-key declines, and the structural errors each raises.

use super::*;
/// Parse the way the runtime does — content in hand, labelled with
/// the `<commit>:<path>` origin (ARCH §2.2). There is no
/// file-loading variant to test.
fn parse(raw: &str) -> Result<PerRepoProviders, LoadError> {
    PerRepoProviders::parse(raw, Path::new("<commit>:providers.yaml"))
}

const ROLES_ONLY: &str = r#"
roles:
  worker:
    provider: anthropic
    model: claude-sonnet-5
    tools: [bash, read_file]
  compactor:
    provider: anthropic
    model: claude-haiku-4-5
"#;

#[test]
fn parses_roles_only() {
    let p = parse(ROLES_ONLY).unwrap();
    assert_eq!(p.roles.len(), 2);
    assert_eq!(p.roles["worker"].provider, "anthropic");
    assert_eq!(p.roles["worker"].model, "claude-sonnet-5");
    // The role's `tools:` list (ARCH §4.3) parses; an omitted list
    // defaults empty (the compactor's toolset is built-in, §2.7).
    assert_eq!(p.roles["worker"].tools, vec!["bash", "read_file"]);
    assert!(p.roles["compactor"].tools.is_empty());
    // An omitted `effort:` is none requested (§4.3) — the general
    // path with empty inputs, not a default level. An omitted
    // `priority:` reads the same way as an explicit `false`: no lane
    // preference (§4.3).
    assert!(p.roles["worker"].effort.is_none());
    assert!(p.roles["worker"].priority.is_none());
}

#[test]
fn parses_the_role_effort_level() {
    let yaml = r#"
roles:
  worker:
    provider: anthropic
    model: claude-sonnet-5
    effort: high
"#;
    let p = parse(yaml).unwrap();
    assert_eq!(p.roles["worker"].effort, Some(Effort::High));
}

#[test]
fn parses_the_role_priority_lane_request() {
    let yaml = r#"
roles:
  worker:
    provider: anthropic
    model: claude-sonnet-5
    priority: true
"#;
    let p = parse(yaml).unwrap();
    assert_eq!(p.roles["worker"].priority, Some(true));
}

#[test]
fn rejects_a_priority_that_is_not_a_checkbox() {
    // `priority:` is a checkbox (§4.3) — a word where a boolean belongs
    // is a structural load error, never a silent none. Decline illegal
    // operations rather than guessing which lane was meant.
    let yaml = r#"
roles:
  worker:
    provider: anthropic
    model: claude-sonnet-5
    priority: fastest
"#;
    let err = parse(yaml).unwrap_err();
    assert!(matches!(err, LoadError::Yaml { .. }));
}

#[test]
fn rejects_an_effort_outside_the_vocabulary() {
    // The vocabulary is closed (`low|medium|high`, ARCH §4.3); a
    // stray spelling is a structural load error, never a silent
    // none — decline illegal operations.
    let yaml = r#"
roles:
  worker:
    provider: anthropic
    model: claude-sonnet-5
    effort: maximal
"#;
    let err = parse(yaml).unwrap_err();
    assert!(matches!(err, LoadError::Yaml { .. }));
}

#[test]
fn missing_roles_section_loads_empty() {
    // A yaml with neither 'roles:' nor any legacy block should parse
    // as an empty map. It is structurally valid and cross-validation
    // is what catches the (likely) real bug — no roles wired.
    let p = parse("# nothing yet\n").unwrap();
    assert!(p.roles.is_empty());
}

#[test]
fn rejects_legacy_providers_block() {
    let yaml = r#"
providers:
  anthropic:
    endpoint: https://api.anthropic.com
    auth: { type: api_key, env: ANTHROPIC_API_KEY }
roles:
  worker:
    provider: anthropic
    model: claude-sonnet-5
"#;
    let err = parse(yaml).unwrap_err();
    match err {
        LoadError::Invalid { key, message, .. } => {
            assert_eq!(key, "providers");
            assert!(message.contains("retired"));
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn rejects_legacy_models_block() {
    let yaml = r#"
models:
  claude-sonnet-5:
    provider: anthropic
    model_id: claude-sonnet-5
    capabilities: []
    context_window: 1000
"#;
    let err = parse(yaml).unwrap_err();
    match err {
        LoadError::Invalid { key, .. } => assert_eq!(key, "models"),
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn rejects_first_legacy_block_seen() {
    // The legacy 'providers' key is checked before 'models', so a
    // file carrying both fails on 'providers' rather than reporting
    // both — one error is enough to send the user back to fix it.
    let yaml = r#"
providers:
  anthropic:
    endpoint: x
    auth: { type: api_key, env: K }
models:
  m: { provider: anthropic, model_id: m, capabilities: [], context_window: 1 }
roles: {}
"#;
    let err = parse(yaml).unwrap_err();
    match err {
        LoadError::Invalid { key, .. } => assert_eq!(key, "providers"),
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn surfaces_yaml_parse_errors() {
    let err = parse("not: [valid: yaml").unwrap_err();
    assert!(matches!(err, LoadError::Yaml { .. }));
}

#[test]
fn non_map_roles_section_surfaces_yaml_error() {
    // `roles:` must be a map of role name → assignment; a sequence
    // is structurally wrong and fails rather than parsing empty.
    let err = parse("roles: [not, a, map]\n").unwrap_err();
    assert!(matches!(err, LoadError::Yaml { .. }));
}

#[test]
fn malformed_role_entry_surfaces_yaml_error() {
    // A role missing the required 'model' field should fail
    // structurally — the per-repo loader does not silently fill in
    // defaults for required fields.
    let yaml = r#"
roles:
  worker:
    provider: anthropic
"#;
    let err = parse(yaml).unwrap_err();
    assert!(matches!(err, LoadError::Yaml { .. }));
}

#[test]
fn top_level_non_mapping_loads_empty_roles() {
    // A scalar at the top level cannot have legacy keys; the loader
    // skips the legacy-block check gracefully and reports an empty
    // roles map (since 'roles' is a missing field on a non-map).
    let p = parse("\"a string\"\n").unwrap();
    assert!(p.roles.is_empty());
}
