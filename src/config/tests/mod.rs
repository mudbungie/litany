//! In-crate integration tests for the config surface (§2.2). Migrated in
//! from `tests/` when the library surface was narrowed to [`crate::cmd`]
//! (§3.4): these exercise the private `config` machinery directly, so
//! they live beside it as `#[cfg(test)]` modules rather than as external
//! integration tests against a now-private API.
//!
//! - [`action_dsl`]: the workflow action-DSL parser.
//! - [`workflow_budgets`]: the `workflow.yaml` `budgets:` block.
//! - [`workflow_yaml`]: `workflow.yaml` parsing and validation.
//! - [`providers_split`]: loading + cross-validating both config halves.
//! - [`schemas_golden`]: the JSON-Schema golden test (replaces the former
//!   `gen-schemas` binary).

mod action_dsl;
mod providers_split;
mod schemas_golden;
mod workflow_budgets;
mod workflow_compaction;
mod workflow_yaml;
