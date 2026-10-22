//! What **governs** a fork: the workflow and the config version a child
//! is dispatched under (ARCH §2.2, §6) — the nearest workflow mark on the
//! dispatcher's descent when one stands, else the followed config commit
//! of the ref the child forks off. Read once, for two readers that must
//! agree: the §6 budget gate (`budgets:`) and the machinery derivation
//! (`procedure::is_machinery`), which decides whether the fork keeps
//! the dispatcher's dialog. Two reads of "which workflow" would be
//! exactly how the ceiling a fork is refused under and the set it is
//! classified by come to disagree.

use super::ChildDispatchRequest;
use crate::config::Workflow;
use crate::config::version::Version;
use crate::prompt::resolve::{VERSION_FILE, workflow_source};
use crate::prompt::{Error, WORKFLOW_FILE};
use crate::template::GitRunner;
use crate::workspace;
use std::path::PathBuf;

/// The workflow governing a child dispatched by `req`, and its version,
/// read from the nearest mark's commit else `commit` (the followed
/// config of the fork ref; the caller derived it).
pub(super) fn governing(
    req: &ChildDispatchRequest<'_>,
    commit: &str,
    git: &dyn GitRunner,
) -> Result<(Workflow, Version), Error> {
    let wf_commit = workflow_source::nearest_mark(req.repo, req.parent_branch, git)
        .map_or_else(|| commit.to_string(), |(_, commit)| commit);
    let read = |file: &str| {
        workspace::show_control(req.repo, &wf_commit, file, git).map_err(|source| {
            Error::ControlRead {
                path: PathBuf::from(format!("{wf_commit}:{file}")),
                source,
            }
        })
    };
    let workflow = Workflow::parse(
        &read(WORKFLOW_FILE)?,
        &PathBuf::from(format!("{wf_commit}:{WORKFLOW_FILE}")),
    )?;
    let version = Version::parse(
        &read(VERSION_FILE)?,
        &PathBuf::from(format!("{wf_commit}:{VERSION_FILE}")),
    )?;
    Ok((workflow, version))
}
