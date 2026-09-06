//! The **history closure** (ARCH §3.3 *the request's referential
//! integrity*): a declaration for every tool the assembled history names
//! that the role's own toolset does not carry.
//!
//! A branch inherits its dispatcher's transcript by fork (§2.3), so the
//! tools that dispatcher used are named in the history whether or not
//! this role was granted them; a provider refuses a history it was not
//! told about, so the array is widened to fit the history rather than the
//! history rewritten to fit the array — transcript entries are immutable
//! and the wire framing is transcript-backed (§2.3, §3.3).
//!
//! **Declaring is not permitting, and the definition now says so**
//! (bl-9c1d). An entry this module adds is one the grant gate will refuse
//! ([`crate::prompt::dispatch::tool_step::permit::refusal`], §3.3): it
//! exists to make the history legible, not to offer the tool. Sent with
//! its committed description and schema it read as an offer, and models
//! took it — across 33 reviewer branches of one lane, `bash` was called
//! **360 times** and refused 360 times, each refusal a full model round
//! trip whose prompt was the entire inherited transcript, and every
//! compactor did the same at its first step. The refusal message was
//! correct and even explained itself; the model tried anyway, because the
//! list it could see said the tool was there.
//!
//! So a non-callable entry carries **the refusal itself as its
//! description** — the same sentence, from the same function, that the
//! door would answer with — and a bare `{"type": "object"}` schema. One
//! home for the sentence, read now before the call instead of after it,
//! and the schema goes too because legibility needs the name, not the
//! shape. A **granted** tool that only reaches the array through the
//! history — granted but undescribed in this tree (§3.3) — is callable,
//! so `refusal` answers `None` for it and it keeps its committed
//! definition; the predicate decides, not this module.

use super::super::Grant;
use super::super::tool_step::permit::refusal;
use super::{entry, tool_name};
use crate::prompt::Error;
use crate::prompt::tool::inject::InjectedTool;
use brazen::{Content, Message, Tool};
use serde_json::{Value, json};

/// Append a declaration for every tool `history` names that `tools` does
/// not already carry (module docs). A name the grant gate would refuse
/// is declared **not callable**: the refusal sentence as its description
/// and an opaque schema.
///
/// The other arm is narrower than it looks. A name that reaches here and
/// IS callable is granted, and a granted name with a committed schema was
/// already elected by [`super::compose`] — so the only callable name left
/// is one the tree describes no schema for, and the stand-in is the whole
/// answer. Its skill description, if the tree carries one, still rides
/// through [`entry`]. That is also the shape of a name the model invented
/// under an empty grant, where `refusal` speaks first.
pub(super) fn close_over(
    worktree: &std::path::Path,
    grant: &Grant<'_>,
    injected: &[InjectedTool],
    tools: &mut Vec<Tool>,
    history: &[Message],
) -> Result<(), Error> {
    for name in referenced(history) {
        if tools.iter().any(|t| tool_name(t) == name) {
            continue;
        }
        if let Some(decline) = refusal(grant.role, grant.tools, injected, &name) {
            tools.push(Tool::Custom {
                name,
                description: Some(decline),
                input_schema: opaque(),
                strict: None,
            });
            continue;
        }
        tools.push(entry(worktree, &name, opaque())?);
    }
    Ok(())
}

/// The stand-in schema: a shape that says nothing, for an entry whose
/// job is to make a name resolvable rather than to describe a call.
fn opaque() -> Value {
    json!({"type": "object"})
}

/// Tool names the `tool_use` blocks of `history` reference, in
/// first-appearance order and deduplicated.
fn referenced(history: &[Message]) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for block in history.iter().flat_map(|m| m.content.iter()) {
        if let Content::ToolUse { name, .. } = block
            && !names.iter().any(|seen| seen == name)
        {
            names.push(name.clone());
        }
    }
    names
}
