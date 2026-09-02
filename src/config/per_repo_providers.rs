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
/// `priority` asks the provider's priority lane for the role's model
/// calls (§4.3); `false` and omitted are one fact — no lane preference,
/// the provider's default lane — so there is no third state to carry.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct RoleAssignment {
    pub provider: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<bool>,
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
mod tests;
