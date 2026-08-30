//! Open-set role validation (ARCH §4.3): a role is valid iff the
//! governing config commit (§2.2) of the ref the child forks off lists
//! `roles.<name>` in `providers.yaml` **and** carries `souls/<name>.md`.
//! Nothing else mints a role and the harness never enumerates role
//! names. That ref is the dispatching branch itself unless the dispatch
//! named a fork point (§2.3), in which case it is the fork point's — the
//! same commit the soul and the `tools:` grant are read from, so the
//! check and the artifacts can never answer to different configs.
//!
//! **Single authoritative home** (`docs/PRINCIPLES.md` Single source of
//! truth): this is the one answer to "is this role dispatchable." Both
//! front doors consult it — the model-facing `dispatch` built-in (§2.5,
//! projecting [`Invalid`] onto its own typed error) and the
//! `litany dispatch <role>` CLI (§3.4, pre-flighting before the fork so
//! a rejected role leaves no branch debris). There is no hard-coded
//! `worker`/`compactor` list anywhere; the closed vocabulary
//! `worker`/`compactor`/`verifier` belongs to the §6 workflow
//! interpreter, not to dispatch validity (§4.3 severability line).

use crate::config::{LoadError, PerRepoProviders};
use crate::prompt::{PER_REPO_PROVIDERS_FILE, SOULS_DIR};
use crate::template::GitRunner;
use crate::workspace;
use std::io;
use std::path::{Path, PathBuf};

/// Why a role is not dispatchable against the config commit that will
/// govern the child. A refusal names the control file the user knows and
/// the agent the dispatch was issued from — never the commit sha and the
/// `<commit>:<path>` git-show form, which are internal representation
/// (`docs/PRINCIPLES.md`; bl-c89b). It says *a child of* that agent
/// rather than *that agent* because the two configs are the same only
/// when the dispatch named no fork point (§2.2 fork-back-in): under
/// `--from` the commit consulted governs the child, not the dispatcher.
#[derive(Debug)]
pub enum Invalid {
    /// The `roles:` block of the governing config's `providers.yaml`
    /// does not list the role. `defined` is the pool that *is* defined,
    /// rendered by [`crate::name::pool`] — the same "name the pool"
    /// idiom `load_skill` and `litany tool` decline with.
    RoleMissing {
        role: String,
        agent: String,
        defined: String,
    },
    /// The role is listed but its soul is absent from the same tree
    /// (§4.3 — the name is the path, no override).
    SoulMissing { role: String, agent: String },
    /// `providers.yaml` parsed but was malformed / legacy (§4.1).
    Config(LoadError),
    /// Deriving the governing config commit (§2.2) or reading a control
    /// file from its tree failed — a defective or absent workspace.
    Governing { branch: String, source: io::Error },
}

impl std::fmt::Display for Invalid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RoleMissing {
                role,
                agent,
                defined,
            } => write!(
                f,
                "role {role:?} is not defined in the providers.yaml that will govern a \
                 child of agent {agent:?} — defined roles: {defined}"
            ),
            Self::SoulMissing { role, agent } => write!(
                f,
                "role {role:?} is defined but its soul {SOULS_DIR}/{role}.md is missing from \
                 the config that will govern a child of agent {agent:?} — a role is its \
                 `roles:` entry and its soul (ARCH §4.3)"
            ),
            Self::Config(e) => write!(f, "providers.yaml: {e}"),
            Self::Governing { branch, source } => {
                write!(f, "governing config for {branch}: {source}")
            }
        }
    }
}

impl std::error::Error for Invalid {}

/// Confirm `role` is dispatchable against the governing config commit of
/// the ref the child will fork off: listed in `providers.yaml` `roles:`
/// **and** carrying `souls/<role>.md` in the same immutable tree (§4.3).
/// Control is read only from the config commit's tree (§2.2), never a
/// worktree file. Both checks precede any fork, so a rejected role leaves
/// no debris.
///
/// `fork_point` is the dispatch's own (`ChildDispatchRequest::fork_point`,
/// §2.3): `None` — the ordinary dispatch, and every model-issued one —
/// forks off `branch` itself, so the question is asked of the parent's
/// config; `Some(ref)` asks it of the config that will actually govern
/// the child. `branch` is the agent the decline speaks of either way.
pub fn validate(
    repo: &Path,
    branch: &str,
    fork_point: Option<&str>,
    role: &str,
    git: &dyn GitRunner,
) -> Result<(), Invalid> {
    let gov = |source| Invalid::Governing {
        branch: branch.to_string(),
        source,
    };
    // The config that will govern the *child*, which is the governing
    // config of the ref it forks off (§2.2 fork-back-in) — the parent's
    // own branch when the dispatch names no fork point. Asking the
    // parent's config for a role the child's config must carry would
    // validate against a commit the soul is not read from.
    let branch_ref = workspace::agent_ref(branch);
    let start = fork_point.unwrap_or(&branch_ref);
    let commit = workspace::governing_config(repo, start, git).map_err(gov)?;
    let providers_raw =
        workspace::show_control(repo, &commit, PER_REPO_PROVIDERS_FILE, git).map_err(gov)?;
    let origin = PathBuf::from(format!("{commit}:{PER_REPO_PROVIDERS_FILE}"));
    let providers = PerRepoProviders::parse(&providers_raw, &origin).map_err(Invalid::Config)?;
    if !providers.roles.contains_key(role) {
        // `roles` is a BTreeMap, so the pool is already in name order.
        let defined: Vec<&str> = providers.roles.keys().map(String::as_str).collect();
        return Err(Invalid::RoleMissing {
            role: role.to_string(),
            agent: branch.to_string(),
            defined: crate::name::pool(&defined),
        });
    }
    let soul_rel = format!("{SOULS_DIR}/{role}.md");
    if !workspace::control_exists(repo, &commit, &soul_rel, git) {
        return Err(Invalid::SoulMissing {
            role: role.to_string(),
            agent: branch.to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::RealGit;
    use crate::workspace::fixture;

    fn git() -> RealGit {
        RealGit::new()
    }

    /// The default scaffold lists `worker` with `souls/worker.md`, so a
    /// worker off a fresh root validates.
    #[test]
    fn a_config_role_with_its_soul_is_valid() {
        let (_h, ws) = fixture::workspace();
        fixture::spawn_root(&ws, "p1");
        validate(&ws, "p1", None, "worker", &git()).unwrap();
    }

    /// A third role the config defines — the v0.7 verifier, zero code —
    /// validates exactly like the template roles.
    #[test]
    fn a_third_config_role_is_valid_zero_code() {
        let (_h, ws) = fixture::workspace();
        let yaml = "roles:\n  worker:\n    provider: anthropic\n    model: sonnet\n  \
                    verifier:\n    provider: anthropic\n    model: sonnet\n";
        fixture::amend_config(
            &ws,
            &[("providers.yaml", yaml), ("souls/verifier.md", "v\n")],
        );
        fixture::spawn_root(&ws, "p9");
        validate(&ws, "p9", None, "verifier", &git()).unwrap();
    }

    #[test]
    fn a_role_absent_from_providers_is_role_missing() {
        let (_h, ws) = fixture::workspace();
        fixture::spawn_root(&ws, "p1");
        let err = validate(&ws, "p1", None, "ghost", &git()).unwrap_err();
        match &err {
            Invalid::RoleMissing {
                role,
                agent,
                defined,
            } => {
                assert_eq!(role, "ghost");
                assert_eq!(agent, "p1");
                assert_eq!(defined, "compactor, worker");
            }
            other => panic!("expected RoleMissing, got {other:?}"),
        }
        // bl-c89b: the product's voice — no commit sha, no `<sha>:path`
        // git-show form — and it names the pool that IS defined.
        assert_eq!(
            err.to_string(),
            "role \"ghost\" is not defined in the providers.yaml that will govern a child \
             of agent \"p1\" \
             — defined roles: compactor, worker"
        );
    }

    #[test]
    fn a_role_listed_without_a_soul_is_soul_missing() {
        let (_h, ws) = fixture::workspace();
        let yaml = "roles:\n  verifier:\n    provider: anthropic\n    model: sonnet\n";
        fixture::amend_config(&ws, &[("providers.yaml", yaml)]);
        fixture::spawn_root(&ws, "p9");
        let err = validate(&ws, "p9", None, "verifier", &git()).unwrap_err();
        match &err {
            Invalid::SoulMissing { role, agent } => {
                assert_eq!(role, "verifier");
                assert_eq!(agent, "p9");
            }
            other => panic!("expected SoulMissing, got {other:?}"),
        }
        assert_eq!(
            err.to_string(),
            "role \"verifier\" is defined but its soul souls/verifier.md is missing from \
             the config that will govern a child of agent \"p9\" — a role is its \
             `roles:` entry and its \
             soul (ARCH §4.3)"
        );
    }

    #[test]
    fn a_legacy_providers_yaml_is_config_error() {
        let (_h, ws) = fixture::workspace();
        fixture::amend_config(&ws, &[("providers.yaml", "providers: {}\n")]);
        fixture::spawn_root(&ws, "p9");
        let err = validate(&ws, "p9", None, "worker", &git()).unwrap_err();
        assert!(matches!(err, Invalid::Config(_)), "{err:?}");
        assert!(err.to_string().starts_with("providers.yaml:"));
    }

    #[test]
    fn a_non_workspace_repo_is_a_governing_error() {
        let holder = tempfile::TempDir::new().unwrap();
        let err = validate(holder.path(), "p1", None, "worker", &git()).unwrap_err();
        match &err {
            Invalid::Governing { branch, .. } => assert_eq!(branch, "p1"),
            other => panic!("expected Governing, got {other:?}"),
        }
        assert!(err.to_string().contains("governing config for p1"));
    }
}
