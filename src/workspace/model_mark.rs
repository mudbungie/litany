//! The agent's **model mark** — `refs/litany/model/<agent-id>` (ARCH
//! §4.3 *The model mark*, bl-246f).
//!
//! A role's model binding — the `(provider row, model id)` pointer and
//! its `effort` / `priority` knobs — lives on the followed config commit
//! (§4.3), and every conversation on a lineage follows the same tip
//! (§2.2, bl-403b). So a pick written through `litany config` moves every
//! conversation on the workspace at once, which is the right reach for
//! an operator moving roles off a dead row and the wrong one for an
//! operator choosing the model *this* conversation should run on. The
//! mark is that fact's per-agent override: a **standing** ref in the
//! shared mark namespace ([`super::MARK_REF_ROOT`]) whose object is a
//! blob carrying a partial role assignment ([`ModelMark`]), read fresh at
//! every step boundary and overlaid on the role's `roles:` entry
//! (`prompt::resolve`). Everything the mark does not carry — `tools`,
//! `max_output_tokens`, the soul, the manifest, `workflow.yaml` — keeps
//! following the lineage.
//!
//! **Content in a blob, not a commit** — unlike the workflow mark
//! ([`super::workflow_mark`]), which selects *which commit answers*. A
//! binding is four scalars whose authoring surface is a dropdown; a
//! config commit per pick would be a lineage per conversation, and a
//! conversation on its own lineage stops following the workspace for
//! every other control fact. The value-carrying precedent is the `cwd`
//! mark ([`super::cwd`]): a ref may name any object, so no second
//! mechanism holds the extra bytes, and `git gc` keeps the blob exactly
//! as long as the mark. The bytes are config-shaped — the same YAML a
//! `roles:` entry speaks, minus the fields a mark may not carry — so the
//! one grammar reads both homes.
//!
//! **No inheritance by descent.** The workflow mark walks the agent's
//! ancestors because a workflow governs a tree; a pick is about the one
//! conversation in front of the operator, and a child's role has its own
//! `roles:` entry. [`read`] answers for exactly the id it is asked.
//!
//! Like the `cwd` mark, the value is staged beside the repo under a
//! pid-unique name for `git hash-object` (the [`GitRunner`] carries no
//! stdin) and removed once hashed; it is never inside a worktree, so no
//! `git add -A` can see it.

use super::{MARK_REF_ROOT, repo_git};
use crate::config::{Effort, LoadError};
use crate::template::GitRunner;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};

/// Ref-namespace prefix for the model mark (§4.3).
pub const MODEL_REF_PREFIX: &str = "model/";

/// `refs/litany/model/<agent-id>` — the mark ref for one agent.
pub fn model_ref(agent_id: &str) -> String {
    format!("{MARK_REF_ROOT}{MODEL_REF_PREFIX}{agent_id}")
}

/// What a mark carries: the fields of a role assignment an operator picks
/// for one conversation (ARCH §4.3). `provider` and `model` are the
/// pointer and are always both present — a mark is a whole binding, never
/// half of one, because the row and the id are only meaningful as a pair
/// (§4.2). `effort` and `priority` are the two knobs the same pick surface
/// writes beside it; absent means what it means on the assignment — none
/// requested, the provider's default — and NOT "keep the lineage's value",
/// so the mark is read without consulting what it replaced.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelMark {
    pub provider: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<bool>,
}

impl ModelMark {
    /// The mark's bytes — one YAML mapping, the `roles:` entry's own
    /// spelling for the fields it carries, so a `git cat-file` of the
    /// ref reads like the config it overlays.
    pub fn to_yaml(&self) -> String {
        // A struct of strings, an enum and a bool cannot fail to
        // serialize; `expect` states the invariant rather than
        // threading an error nobody can act on.
        serde_yaml_ng::to_string(self).expect("a model mark is a plain mapping of scalars")
    }

    /// Parse a mark's bytes. The error is the YAML reader's own, labelled
    /// by the caller with the ref it came from.
    pub fn parse(raw: &str) -> Result<Self, serde_yaml_ng::Error> {
        serde_yaml_ng::from_str(raw)
    }
}

/// The agent's standing model mark, or `None` when none is set — the
/// ordinary state of every agent, not an error. An unreadable mark (no
/// repo, a ref at a non-blob, a git that would not run) reads the same
/// way: the role's own binding applies, and no step fails for want of a
/// mark it does not have. Bytes that *are* there but do not parse are a
/// different fact — a ref somebody wrote by hand — and come back as the
/// config error they are, labelled with the ref as a control file is
/// labelled with its commit, because resolving the lineage's binding
/// over a mark the operator believes stands would be the silent
/// fallback §4.3 refuses.
pub fn read(
    workspace: &Path,
    agent_id: &str,
    git: &dyn GitRunner,
) -> Result<Option<ModelMark>, LoadError> {
    let spec = model_ref(agent_id);
    let Ok(out) = git.run_capture(&repo_git(workspace), &["cat-file", "blob", &spec]) else {
        return Ok(None);
    };
    if out.trim().is_empty() {
        return Ok(None);
    }
    ModelMark::parse(&out)
        .map(Some)
        .map_err(|source| LoadError::Yaml {
            path: PathBuf::from(spec),
            source,
        })
}

/// Set the agent's model mark — last write wins, so picking again is
/// just marking again. Writes the value as a blob and points the mark at
/// it (module docs on the staging).
pub fn write(
    workspace: &Path,
    agent_id: &str,
    mark: &ModelMark,
    git: &dyn GitRunner,
) -> io::Result<()> {
    let repo = repo_git(workspace);
    let staged = repo.join(format!("model-mark.{}.tmp", std::process::id()));
    std::fs::write(&staged, mark.to_yaml())?;
    let staged_str = staged.to_string_lossy().into_owned();
    let hashed = git.run_capture(&repo, &["hash-object", "-w", "--", &staged_str]);
    std::fs::remove_file(&staged)?;
    git.run(&repo, &["update-ref", &model_ref(agent_id), &hashed?])
}

/// Delete the mark: from the agent's next step boundary on, its role's
/// own `roles:` entry on the followed commit binds the model again —
/// removing the mark deletes config, never code (`docs/PRINCIPLES.md`
/// severability).
pub fn clear(workspace: &Path, agent_id: &str, git: &dyn GitRunner) -> io::Result<()> {
    git.run(
        &repo_git(workspace),
        &["update-ref", "-d", &model_ref(agent_id)],
    )
}

#[cfg(test)]
mod tests;
