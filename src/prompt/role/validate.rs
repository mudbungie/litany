//! Open-set role validation (ARCH §4.3): a role is valid iff a config
//! commit (§2.2) lists `roles.<name>` in `providers.yaml` **and** carries
//! `souls/<name>.md`. Nothing else mints a role and the harness never
//! enumerates role names.
//!
//! **Single authoritative home** (`docs/PRINCIPLES.md` Single source of
//! truth): this is the one answer to "is this role resolvable against
//! that config." Three front doors consult it, each naming the commit
//! its own act will read the soul and the grant from, so the check and
//! the artifacts can never answer to different configs:
//!
//! - the model-facing `dispatch` built-in (§2.5, projecting [`Invalid`]
//!   onto its own typed error) and the `litany dispatch <role>` CLI
//!   (§3.4) ask [`validate`], whose commit is the governing config of
//!   the ref the child forks off — the dispatching branch itself unless
//!   the dispatch named a fork point (§2.3);
//! - `litany retarget --role` (§2.2, bl-946c) asks [`against`] directly,
//!   because its commit is the retarget target and is already in hand.
//!
//! `litany prompt --role` consults neither: a fresh root's role is
//! *resolved* before the fork ([`crate::prompt::resolve`]), which reads
//! the same two files and declines on the same two absences, so a second
//! check there would be a second answer to a question already asked.
//!
//! There is no hard-coded `worker`/`compactor` list anywhere; the closed
//! vocabulary `worker`/`compactor`/`verifier` belongs to the §6 workflow
//! interpreter, not to role validity (§4.3 severability line).

use crate::config::{LoadError, PerRepoProviders};
use crate::prompt::{PER_REPO_PROVIDERS_FILE, SOULS_DIR};
use crate::template::GitRunner;
use crate::workspace;
use std::io;
use std::path::{Path, PathBuf};

/// Why a role is not resolvable against the config commit in question.
/// A refusal names the control file the user knows and the **subject**
/// — what that commit is about to govern — never the commit sha and the
/// `<commit>:<path>` git-show form, which are internal representation
/// (`docs/PRINCIPLES.md`; bl-c89b). The subject is the caller's phrase
/// because only the caller knows it: a dispatch says *a child of* the
/// agent rather than the agent itself, since the two configs are the
/// same only when the dispatch named no fork point (§2.2 fork-back-in),
/// while a retarget says the agent, which is exactly who is moving.
#[derive(Debug)]
pub enum Invalid {
    /// The `roles:` block of the governing config's `providers.yaml`
    /// does not list the role. `defined` is the pool that *is* defined,
    /// rendered by [`crate::name::pool`] — the same "name the pool"
    /// idiom `load_skill` and `litany tool` decline with.
    RoleMissing {
        role: String,
        subject: String,
        defined: String,
    },
    /// The role is listed but its soul is absent from the same tree
    /// (§4.3 — the name is the path, no override).
    SoulMissing { role: String, subject: String },
    /// `providers.yaml` parsed but was malformed / legacy (§4.1).
    Config(LoadError),
    /// Deriving the governing config commit (§2.2) or reading a control
    /// file from its tree failed — a defective or absent workspace.
    Governing { subject: String, source: io::Error },
}

impl std::fmt::Display for Invalid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RoleMissing {
                role,
                subject,
                defined,
            } => write!(
                f,
                "role {role:?} is not defined in the providers.yaml that will govern \
                 {subject} — defined roles: {defined}"
            ),
            Self::SoulMissing { role, subject } => write!(
                f,
                "role {role:?} is defined but its soul {SOULS_DIR}/{role}.md is missing from \
                 the config that will govern {subject} — a role is its `roles:` entry and \
                 its soul (ARCH §4.3)"
            ),
            Self::Config(e) => write!(f, "providers.yaml: {e}"),
            Self::Governing { subject, source } => {
                write!(f, "governing config for {subject}: {source}")
            }
        }
    }
}

impl std::error::Error for Invalid {}

/// Confirm `role` is dispatchable against the governing config commit of
/// the ref the child will fork off (§2.2 fork-back-in). Both checks
/// precede any fork, so a rejected role leaves no debris.
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
    let subject = format!("a child of agent {branch:?}");
    // Asking the parent's config for a role the child's config must
    // carry would validate against a commit the soul is not read from.
    let branch_ref = workspace::agent_ref(branch);
    let start = fork_point.unwrap_or(&branch_ref);
    // Followed, not frozen (§2.2, bl-403b): validity is asked of the
    // same commit the soul and grant will be read from.
    let commit = workspace::current_config::current_config(repo, start, git)
        .map_err(|source| Invalid::Governing {
            subject: subject.clone(),
            source,
        })?
        .commit()
        .to_string();
    against(repo, &commit, &subject, role, git)
}

/// Confirm `role` is resolvable against the config commit `commit`:
/// listed in `providers.yaml` `roles:` **and** carrying
/// `souls/<role>.md` in the same immutable tree (§4.3). Control is read
/// only from the config commit's tree (§2.2), never a worktree file.
/// `subject` names what that commit is about to govern, for the decline.
///
/// The commit is the caller's, because a caller that already holds one
/// must not have it re-derived: `litany retarget --role` validates the
/// **target** it is about to mark, which is a commit no ancestry query
/// of the agent's branch answers yet (bl-946c).
pub fn against(
    repo: &Path,
    commit: &str,
    subject: &str,
    role: &str,
    git: &dyn GitRunner,
) -> Result<(), Invalid> {
    let providers_raw = workspace::show_control(repo, commit, PER_REPO_PROVIDERS_FILE, git)
        .map_err(|source| Invalid::Governing {
            subject: subject.to_string(),
            source,
        })?;
    let origin = PathBuf::from(format!("{commit}:{PER_REPO_PROVIDERS_FILE}"));
    let providers = PerRepoProviders::parse(&providers_raw, &origin).map_err(Invalid::Config)?;
    if !providers.roles.contains_key(role) {
        // `roles` is a BTreeMap, so the pool is already in name order.
        let defined: Vec<&str> = providers.roles.keys().map(String::as_str).collect();
        return Err(Invalid::RoleMissing {
            role: role.to_string(),
            subject: subject.to_string(),
            defined: crate::name::pool(&defined),
        });
    }
    let soul_rel = format!("{SOULS_DIR}/{role}.md");
    if !workspace::control_exists(repo, commit, &soul_rel, git) {
        return Err(Invalid::SoulMissing {
            role: role.to_string(),
            subject: subject.to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests;
