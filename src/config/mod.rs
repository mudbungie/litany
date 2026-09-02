//! Configuration files for a conversation repo.
//!
//! Each submodule owns one file per ARCH §2.2 — see [`version`],
//! [`models`] (global, at the harness root — the optional `adapter:`
//! override), [`per_repo_providers`] (per-repo `roles:` section),
//! [`manifest`], [`workflow`]. [`cross`] enforces references across
//! files: a workflow's `dispatch(<role>)` action must name a role
//! declared in the per-repo `roles:` section.

#[cfg(test)]
mod tests;

pub mod action;
pub mod cross;
pub mod effort;
pub mod error;
pub mod manifest;
pub mod models;
pub mod per_repo_providers;
pub mod schemas;
pub mod tool_control;
pub mod tool_output;
pub mod version;
pub mod workflow;

pub use action::Action;
pub use effort::Effort;
pub use error::LoadError;
pub use models::Models;
pub use per_repo_providers::PerRepoProviders;
pub use tool_control::ToolControl;
pub use tool_output::ToolOutputBound;
pub use workflow::{Budgets, CompactionConfig, CompactionTrigger, Event, RetryConfig, Workflow};

use std::path::Path;

/// The two halves of the model configuration loaded together: the
/// global `models.yaml` (the optional `adapter:` override — owned by
/// the harness root, ARCH §4.2) and the per-repo `roles:` section
/// (frozen at conversation creation, ARCH §4.3). The role assignment is
/// the single home of the (provider row, model id) pointer; id validity
/// is brazen's fact, caught at the first live model call (§4.2).
#[derive(Debug)]
pub struct ModelsConfig {
    pub global: Models,
    pub per_repo: PerRepoProviders,
}

impl ModelsConfig {
    /// Load the global half from disk and the per-repo half from content
    /// already in hand — the governing-config read path (ARCH §2.2:
    /// `providers.yaml` is read from the config commit's tree, never
    /// from a worktree file). `per_repo_origin` labels per-repo parse
    /// errors (e.g. `<config-commit>:providers.yaml`).
    pub fn load_with_per_repo(
        global_path: &Path,
        per_repo_raw: &str,
        per_repo_origin: &Path,
    ) -> Result<Self, LoadError> {
        let global = Models::load(global_path)?;
        let per_repo = PerRepoProviders::parse(per_repo_raw, per_repo_origin)?;
        Ok(Self { global, per_repo })
    }
}
