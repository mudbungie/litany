//! Where a fresh root agent forks off (ARCH §2.3 *Any ref is a legal
//! fork point*).
//!
//! Starting an agent *is* creating a branch off a ref (§2.3), so a start
//! names exactly one ref. `litany prompt` spells that name two ways, and
//! [`resolve`] is the one place either spelling becomes the ref:
//!
//! - `--config <name>` — the head of the `config/<name>` lineage (§2.3
//!   *Fresh start*). The bare name is the vocabulary `litany config`
//!   already uses; the `config/` prefix is applied at the git boundary
//!   ([`workspace::config_ref`]), never typed by a user.
//! - `--from <ref>` — any ref at all, verbatim: a historical commit of
//!   any agent (fork-back-in, §7.2), a stopped agent's tip (§2.9), a
//!   config commit. There is no special prefix and no distinct
//!   operation — provenance is the ancestry (§7.2), and the governing
//!   lineage is derived from that ancestry like any other agent's and
//!   followed to its tip ([`workspace::current_config`], §2.2 bl-403b),
//!   so **fork chooses the lineage** whatever the fork point is.
//!
//! Naming neither is not a special case: it is `--config default`
//! ([`workspace::DEFAULT_CONFIG_NAME`]) — the general path with empty
//! inputs. Naming both is declined: one start, one fork point. The
//! decline is the *library's*, not clap's, because both bindings (§3.4)
//! construct the same `Args` and only one of them parses argv.

use crate::template::GitRunner;
use crate::workspace;
use std::path::Path;

/// Why a start needed the ref, for the shared [`workspace::require_ref`]
/// decline (§2.3).
const REASON: &str = "a root agent forks off the ref you name (ARCH §2.3, ARCH §7.2)";

/// A fork point that could not be resolved to one ref.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Both spellings were given. They name the same thing — the one ref
    /// the start forks off — so there is nothing to reconcile and
    /// nothing to guess (PRINCIPLES "Decline illegal operations").
    #[error(
        "pass --from <ref> or --config <name>, not both — a start forks off exactly one \
         ref (ARCH §2.3); --config <name> is the head of config/<name>, --from <ref> is \
         any ref at all"
    )]
    Conflict,
    #[error(transparent)]
    UnknownRef(#[from] workspace::UnknownRef),
    #[error(transparent)]
    UnknownLineage(#[from] workspace::UnknownLineage),
}

/// Resolve `--from` / `--config` to the ref the root agent forks off,
/// declining an absent one before any branch, worktree or inbox exists.
pub fn resolve(
    repo: &Path,
    from: Option<&str>,
    config: Option<&str>,
    git: &dyn GitRunner,
) -> Result<String, Error> {
    match (from, config) {
        (Some(_), Some(_)) => Err(Error::Conflict),
        (Some(rev), None) => {
            workspace::require_ref(repo, rev, REASON, git)?;
            Ok(rev.to_owned())
        }
        (None, name) => {
            let name = name.unwrap_or(workspace::DEFAULT_CONFIG_NAME);
            workspace::require_lineage(repo, name, git)?;
            Ok(workspace::config_ref(name))
        }
    }
}

#[cfg(test)]
mod tests;
