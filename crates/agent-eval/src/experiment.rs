//! Resolving an experiment (ARCH §9.3).
//!
//! An experiment is a `workflow.yaml` variant under `experiments/<name>/`
//! — a config diff, no code changes. `--config <name>` names the
//! subdirectory; the runner resolves it to that directory's
//! `workflow.yaml`, which the agent invocation is handed as the config
//! to run under. A missing variant is a loud failure, never a silent
//! fallback (`docs/PRINCIPLES.md` Decline illegal operations).
//!
//! The `baseline` experiment is the shipped default *itself*: an empty
//! diff has nothing to store, so `experiments/baseline/workflow.yaml`
//! symlinks `template/workflow.yaml` (see `experiments/README.md`). That
//! is a fact about the repo's experiments directory, not about this
//! resolver — one file under two names resolves like any other, so
//! "the baseline is the template" costs no code here.

use std::path::{Path, PathBuf};

/// A resolved experiment: its name and the `workflow.yaml` that defines
/// it.
#[derive(Clone, Debug, PartialEq)]
pub struct Experiment {
    pub name: String,
    pub workflow: PathBuf,
}

/// Every way [`resolve`] can fail.
#[derive(Debug, thiserror::Error)]
pub enum ExperimentError {
    #[error("experiment {name:?} has no workflow.yaml at {path}")]
    Missing { name: String, path: PathBuf },
}

/// Resolve `config` to `<experiments_root>/<config>/workflow.yaml`,
/// erroring if that file does not exist.
///
/// The path is canonicalized: it rides to the harness driver as
/// `LITANY_EXPERIMENT` (the `agent` seam), and the driver runs with the
/// per-run working directory as its cwd — a root-relative path like
/// `experiments/baseline/workflow.yaml` would name nothing from there.
/// Canonicalizing also walks the baseline's symlink to the template,
/// which resolves the same one file it names.
pub fn resolve(config: &str, experiments_root: &Path) -> Result<Experiment, ExperimentError> {
    let named = experiments_root.join(config).join("workflow.yaml");
    match named.canonicalize() {
        Ok(workflow) if workflow.is_file() => Ok(Experiment {
            name: config.to_string(),
            workflow,
        }),
        _ => Err(ExperimentError::Missing {
            name: config.to_string(),
            path: named,
        }),
    }
}
