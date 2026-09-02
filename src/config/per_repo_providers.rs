//! Per-repo `providers.yaml` — role → (provider, model) assignments
//! frozen at conversation creation (ARCH §4.3).
//!
//! The conversation-repo file carries only the `roles:` section: which
//! provider row name and which model id each role dispatches to — the
//! single home of that pointer (bl-35e2). Endpoint and auth resolve
//! inside brazen at call time (never a harness file, ARCH §4.1); model
//! id validity is the wire's fact, caught at the first live model call
//! (§4.2).
//!
//! A legacy `providers:` or `models:` block (the v0.2 shape) is a hard
//! load error: neither section exists any more — provider rows are
//! brazen's config, and the global models table is retired (bl-35e2) —
//! so a per-repo file carrying one is structurally wrong rather than
//! just noisy. (Phase 1 of the v0.3 layout migration warned; Phase 4
//! escalated to error once the v0.2 template was retired.)

use crate::config::effort::Effort;
use crate::config::error::LoadError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// Top-level shape of the per-repo `providers.yaml`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
pub struct PerRepoProviders {
    #[serde(default)]
    pub roles: BTreeMap<String, RoleAssignment>,
}

/// One role's assignment: which provider (by brazen row name) and which
/// model (by wire id), plus the role's enabled tools (ARCH §4.3). This
/// pointer is the whole model binding (bl-35e2) — no global table
/// mediates it; id validity is caught at the first live model call
/// (§4.2). Endpoint and auth resolve inside brazen at call time (§4.1 —
/// no `auth_env` / `endpoint_env` here). `tools` selects which tools
/// the role's agent may call (§3.3); omitted or empty means none.
/// `effort` is the role's reasoning-effort level ([`Effort`], §4.3);
/// omitted means none requested — the general path with empty inputs.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct RoleAssignment {
    pub provider: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,
}

const LEGACY_KEYS: &[&str] = &["providers", "models"];

impl PerRepoProviders {
    /// Parse `providers.yaml` content already in hand — the
    /// governing-config read path (ARCH §2.2: control is read from the
    /// config commit's tree, never from a worktree file). `origin`
    /// labels errors (e.g. `<config-commit>:providers.yaml`).
    pub fn parse(raw: &str, path: &Path) -> Result<Self, LoadError> {
        let doc: serde_yaml_ng::Value =
            serde_yaml_ng::from_str(raw).map_err(|source| LoadError::Yaml {
                path: path.to_path_buf(),
                source,
            })?;

        if let Some(map) = doc.as_mapping() {
            for legacy in LEGACY_KEYS {
                if map.contains_key(*legacy) {
                    return Err(LoadError::Invalid {
                        path: path.to_path_buf(),
                        key: (*legacy).to_string(),
                        message: format!(
                            "{legacy:?} block is retired: provider rows are \
                             brazen's config and models are named directly on \
                             roles; the per-repo file must only carry the \
                             'roles:' section (ARCH §4.1, §4.3)",
                        ),
                    });
                }
            }
        }

        let roles_value = doc
            .as_mapping()
            .and_then(|m| m.get("roles"))
            .cloned()
            .unwrap_or(serde_yaml_ng::Value::Null);
        let roles: BTreeMap<String, RoleAssignment> = if roles_value.is_null() {
            BTreeMap::new()
        } else {
            serde_yaml_ng::from_value(roles_value).map_err(|source| LoadError::Yaml {
                path: path.to_path_buf(),
                source,
            })?
        };

        Ok(Self { roles })
    }
}

#[cfg(test)]
mod tests {
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
        // path with empty inputs, not a default level.
        assert!(p.roles["worker"].effort.is_none());
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
}
