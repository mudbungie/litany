//! The **door**: the gates one inner invocation passes before it runs
//! (`docs/DESIGN_CODE_EXECUTION.md` §2.1, ARCH §3.3 *The multi-tool*
//! No bypass).
//!
//! An **inner invocation** is a `{name, input}` the model minted no wire
//! id for, run under a derived id through the same controls a top-level
//! `tool_use` meets (`docs/TAXONOMY.md` §4). One surface raises one
//! today — the `litany invoke` verb ([`cli`]), which a program reaches
//! one invocation at a time — and the multi-tool's envelope, whose list
//! was written ahead of time, retired into it
//! (`docs/DESIGN_CODE_EXECUTION.md` §5). The gate stayed where it is
//! because it is the *door's* rule, not the composer's: whatever raises
//! an inner invocation next calls [`gate`] rather than restating it.
//!
//! The order is the tool window's own: the depth refusal, the grant
//! gate ([`super::tool_step::permit::refusal`]), then the tool-control
//! seam ([`super::tool_step::seam`]). Everything after — the executor,
//! the diagnostic record, the result envelope — is the executor's, and
//! is the same code for both surfaces already.
//!
//! **A hold cannot park an inner invocation.** A top-level hold writes
//! the hold mark and parks the branch, to be re-adjudicated on the next
//! drive. An inner one cannot: whatever composed it has already run
//! program statements whose side effects a park would make re-run on
//! resume — exactly what the mark exists to rule out. So it
//! degrades to an in-band decline that keeps the hold's one guarantee
//! (the invocation does not run unreviewed) and tells the model to
//! re-issue it top-level, where a hold parks properly.

pub(crate) mod caller;
pub(crate) mod cli;

use super::tool_step::{permit, seam};
use crate::config::ToolControl;
use crate::prompt::tool::inject::InjectedTool;
use serde_json::Value;
use std::path::Path;
use std::sync::atomic::AtomicBool;

/// The tools that compose invocations of their own, and so may not be
/// one (§2.7, ARCH §3.3 *Depth 1*). Nesting buys no expressive power —
/// a program can already loop — and it would compound id derivation and
/// attribution for nothing. One name since the multi-tool retired into
/// the program (`docs/DESIGN_CODE_EXECUTION.md` §5); it stays a list
/// because the rule is about a *class* of tool, not about `python`.
pub(crate) const COMPOSING: [&str; 1] = [crate::prompt::tool::builtin::PYTHON];

/// One invocation and everything the gates adjudicate it on. `id` is
/// the id it will run under — the caller's own, which for a program's
/// stub module is the derived `<tool-id>-<k>` — so the control sees the
/// same id the record lands under.
pub(crate) struct Passage<'a> {
    pub(crate) id: &'a str,
    pub(crate) name: &'a str,
    pub(crate) input: &'a Value,
    pub(crate) role: &'a str,
    /// The role's `providers.yaml` `tools:` grant (ARCH §4.3).
    pub(crate) grant: &'a [String],
    /// Everything injected into this request — the composer reads the
    /// same list, which is what keeps declaring and permitting from
    /// drifting (ARCH §3.3 *declaring is not permitting*).
    pub(crate) injected: &'a [InjectedTool],
    pub(crate) tool_control: Option<&'a ToolControl>,
    pub(crate) conv_repo: &'a Path,
    pub(crate) conv_id: &'a str,
    pub(crate) stop: &'a AtomicBool,
}

/// What the gates decided. `None` from [`gate`] is the §2.9 stop
/// observed mid-consult — the same reading the tool window gives it.
pub(crate) enum Verdict {
    /// Settled without reaching the executor; the text is the in-band
    /// decline the model reads.
    Declined(String),
    /// Cleared to run.
    Proceed,
}

/// Run one inner invocation's gates, stopping short of execution so a
/// `parallel` envelope can clear every entry before any of them runs
/// and the door verb can execute on its own terms.
pub(crate) fn gate(p: &Passage<'_>) -> Result<Option<Verdict>, crate::prompt::Error> {
    if COMPOSING.contains(&p.name) {
        return Ok(Some(Verdict::Declined(depth_decline(p.name))));
    }
    if let Some(decline) = permit::refusal(p.role, p.grant, p.injected, p.name) {
        return Ok(Some(Verdict::Declined(decline)));
    }
    match seam::adjudicate(
        p.tool_control,
        p.role,
        p.id,
        p.name,
        p.input,
        p.conv_repo,
        p.conv_id,
        p.stop,
    )? {
        seam::Gate::Stopped => Ok(None),
        seam::Gate::Refuse(reason) => {
            Ok(Some(Verdict::Declined(seam::refusal_text(p.name, &reason))))
        }
        seam::Gate::Hold(reason) => Ok(Some(Verdict::Declined(hold_decline(p.name, &reason)))),
        seam::Gate::Proceed => Ok(Some(Verdict::Proceed)),
    }
}

/// The depth-1 decline: the named tool composes invocations of its own,
/// so it is not one to compose.
fn depth_decline(name: &str) -> String {
    format!(
        "{name:?} composes tool invocations of its own and may not be one \
         (depth 1): issue its invocations directly."
    )
}

/// The degraded hold (this module's header): the invocation did not
/// run, and the model is told where a hold can park.
fn hold_decline(name: &str, reason: &str) -> String {
    format!(
        "the workflow's tool control held {name:?} ({reason}), and a hold cannot park \
         an invocation another tool is composing: re-issue this invocation as a \
         top-level tool_use to have it reviewed (ARCH §3.3 Tool control)."
    )
}
