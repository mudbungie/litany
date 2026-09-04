//! The `land_compaction` action of the §6 binding interpreter (ARCH
//! §2.6, §2.7) — the compaction landing as the child-result interpreter
//! reaches it, beside [`super::flush`] (which *starts* a compaction pass)
//! and [`super::verifier`] (which owns the gate).
//!
//! Two facts live here and nowhere else: which compactor return
//! qualifies to land ([`qualifies`]), and what a landing that did not
//! simply succeed says to the operator ([`land`]). The interpreter above
//! decides only *whether* the action fires.

use super::ChildResult;
use crate::config::Workflow;
use crate::prompt::notice::notice;
use crate::prompt::{Error, compactor, inbox};
use crate::template::GitRunner;
use std::path::Path;

/// Does this return qualify for a **landing** — the compaction landing
/// here, or the reviewer's [`super::proposal::stage`]? Only a
/// `final-response` epitaph does (§2.6/§2.7 — a child that ends on any
/// other epitaph lands nothing): the epitaph is the pinned manner of
/// ending, and code branches on its value (§2.6). One gate for both,
/// because it is one question: did the pass this child was forked to
/// perform actually finish?
pub(super) fn qualifies(cr: &ChildResult) -> bool {
    cr.epitaph == inbox::Epitaph::FinalResponse.as_str()
}

/// The workflow's cap on the landing's extract (ARCH §2.7, §6) — the one
/// place the landing's config key is named, so the interpreter above
/// hands over the `workflow.yaml` it already holds rather than a number
/// it had to know to read. Absent block, absent key: no extract.
fn extract_bytes(workflow: &Workflow) -> Option<usize> {
    workflow
        .compaction
        .as_ref()
        .and_then(|c| c.intermediate.extract_bytes)
}

/// `land_compaction` (§2.6): land the returning compactor's product by
/// rebase-forward — the compaction base plus the replayed live tail —
/// then consume the trigger message (the base commit is the record —
/// never a transcript entry).
///
/// A replay git could not resolve is **declined** by [`compactor::land`]
/// — aborted and marked at `refs/litany/conflicted/<compactor-id>` — and
/// reported here for the operator; a pass another landing overtook is
/// **superseded** and reported without a mark (not a defect — the next
/// checkpoint trigger fires afresh). Both reports are operator notices
/// (`crate::prompt::notice`) and carry its prefix: a driver's stderr is
/// captured, so the reader is a program (§2.11). The trigger message is
/// consumed in every case: the compactor has returned, and re-reading
/// its result would re-attempt a landing whose outcome is already
/// recorded.
pub(super) fn land(
    worktree: &Path,
    agent_id: &str,
    cr: &ChildResult,
    workflow: &Workflow,
    git: &dyn GitRunner,
) -> Result<(), Error> {
    match compactor::land(
        worktree,
        agent_id,
        &cr.child_id,
        extract_bytes(workflow),
        git,
    )? {
        compactor::LandOutcome::Conflicted(paths) => notice!(
            "compaction landing [{}] declined — git could not replay {} \
             (marked refs/litany/conflicted/{}, ARCH §2.6); the branch continues uncompacted",
            cr.child_id,
            paths.join(", "),
            cr.child_id,
        ),
        compactor::LandOutcome::Superseded => notice!(
            "compaction landing [{}] superseded — a compaction landed since \
             its fork point (ARCH §2.6); the branch continues",
            cr.child_id,
        ),
        compactor::LandOutcome::Landed | compactor::LandOutcome::NoOp => {}
    }
    std::fs::remove_file(&cr.path).map_err(Error::Io)
}
