//! **A step may not advance the lineage its own conversation runs on**
//! (ARCH §3.3 *Environment*, `docs/DESIGN_LEARNING_LOOP.md` §3).
//!
//! The config lineage a conversation is born on holds everything that
//! governs it — `souls/*.md`, `providers.yaml`'s model and grant rows,
//! `facts.md`, the workspace skills. Two acts advance one: the operator's
//! `litany config`, and `litany proposal --accept`, which is the operator
//! agreeing to a proposal. Both are the *operator's*, and the learning
//! loop is built on that: a reviewer proposes onto a branch no lineage
//! points at, and acceptance is the veto's other half (§3 *One writer per
//! branch holds*).
//!
//! Nothing enforced it. A step with the shipped `bash` grant finds
//! `litany` on its PATH, writes a script, exports it as `$EDITOR` and
//! runs `litany config <workspace>` — and the lineage advances, souls,
//! grants, models and facts included, from inside the conversation those
//! very files govern (yog bl-baed, round-1 triage ruling 3). The
//! staged-proposal veto is walkable while that door is open, so it is
//! not a veto.
//!
//! **The marker already exists.** Every tool invocation carries
//! `LITANY_TOOL_ID` — the `tool_use.id` being executed — set by the
//! executor on every spawn since bl-e8d7, owed by a routing host on any
//! spawn it makes (ARCH §3.3). It is present exactly when a process is
//! a tool invocation of a running step, which is exactly the condition
//! these two verbs must refuse under, so nothing new is signalled and no
//! flag is added: the guard reads the signal that is already there.
//!
//! **The refusal names the lawful route**, because the agent that hits
//! it is trying to do something reasonable and the design has a place
//! for it: a step *proposes*, and an operator accepts.
//!
//! Read the marker where every other process-global is read — off
//! [`crate::cmd::Fx`], filled once at the binding (§3.4). Not
//! `std::env::var_os` inside the verb: the environment is per-process
//! and the test binary runs its beats in threads, so a sibling beat
//! setting the contract vars for its own run would decide whether an
//! unrelated `config` was refused (the bl-b5b1 reasoning, `Fx.conv_branch`).

use std::ffi::OsStr;

/// A lineage-advancing verb reached from inside a step — refused,
/// rendered inside the verb's uniform `litany <verb>: <error>` failure
/// line (§3.4).
#[derive(Debug, thiserror::Error)]
#[error(
    "{act} from inside a step: LITANY_TOOL_ID is set, so this process is a tool invocation of \
     a running conversation (ARCH §3.3). The config lineage a conversation runs on is advanced \
     by the operator, or by a proposal the operator accepted — never from inside a step \
     (docs/DESIGN_LEARNING_LOOP.md §3). Stage the change as a proposal and say so in your \
     answer; `litany proposal <workspace>` is where the operator reads and accepts it"
)]
pub struct FromInsideAStep {
    /// The act refused, as the sentence's subject — "advance a config
    /// lineage", "accept a proposal onto its lineage".
    act: &'static str,
}

/// Decline `act` when `tool_id` says this process is a tool invocation.
///
/// Set **and non-empty** is the test, the same reading
/// [`crate::prompt::inbox::resolve_cli_sender`] gives the other contract
/// variable: an exported-but-empty variable is an environment artifact,
/// not a claim to be one of those invocations.
pub fn require_operator(act: &'static str, tool_id: Option<&OsStr>) -> Result<(), FromInsideAStep> {
    match tool_id {
        Some(id) if !id.is_empty() => Err(FromInsideAStep { act }),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::require_operator;
    use std::ffi::OsStr;

    #[test]
    fn an_absent_or_empty_marker_is_the_operator() {
        assert!(require_operator("advance a config lineage", None).is_ok());
        assert!(require_operator("advance a config lineage", Some(OsStr::new(""))).is_ok());
    }

    #[test]
    fn a_set_marker_is_refused_and_the_refusal_names_the_route() {
        let err =
            require_operator("advance a config lineage", Some(OsStr::new("toolu_01"))).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("advance a config lineage"), "{msg}");
        assert!(msg.contains("LITANY_TOOL_ID"), "{msg}");
        assert!(msg.contains("litany proposal <workspace>"), "{msg}");
    }
}
