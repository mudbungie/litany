//! Workflow actions: a small DSL embedded in YAML strings.
//!
//! Examples (from `docs/ARCHITECTURE.md` §6):
//! - `dispatch(worker)`
//! - `dispatch(worker, with: verifier.feedback)`
//! - `dispatch(compactor, mode: intermediate)`
//! - `gate_return_on(verifier.approve)`
//! - `deliver_result`
//! - `land_compaction`
//! - `stage_proposal`
//! - `mark_abandoned`
//! - `notify_ui`
//!
//! The closed action set is enumerated by [`Action`]; arity and named-arg
//! validity is enforced when parsing the source string.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// One workflow action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    Dispatch {
        role: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        with: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mode: Option<DispatchMode>,
    },
    /// Hold a worker's result delivery until the gating predicate holds
    /// (ARCH §6 "gate_return_on — the delivery-hold"): the held state is a
    /// disk query, never a stored flag.
    GateReturnOn {
        predicate: String,
    },
    /// Deliver a (possibly gate-held) result message + work-product
    /// transfer (ARCH §2.6). Lifts a `gate_return_on` hold on approval.
    DeliverResult,
    /// Land a returning compactor's product by rebase-forward (ARCH
    /// §2.6): squash the compaction span into a compaction base and
    /// replay the live tail on top, at a step boundary. Bound to
    /// `compactor_return`. The retired spelling `compaction_merge` still
    /// parses to this action — the merge-back mechanism it named is gone
    /// (bl-bc9c), but configs are frozen commits (§2.2) and a running
    /// workspace's vocabulary must keep resolving.
    LandCompaction,
    /// Land a returning reviewer's product as a **proposal**: one config
    /// commit on `proposal/<reviewer-id>`, parented on the followed
    /// config commit the reviewer read, whose diff is the reviewer's own
    /// edits and whose message is its terminal response
    /// (`docs/DESIGN_LEARNING_LOOP.md` §3). Bound to `reviewer_return`
    /// and epitaph-gated like [`Action::LandCompaction`]; the return is
    /// consumed, never delivered, so a proposed skill or facts patch
    /// reaches no lineage until `litany proposal --accept` fast-forwards
    /// it. **Vocabulary only today** (bl-30fe): it parses, and the
    /// interpreter declines it with `ActionUnsupported` until the
    /// landing ships (`docs/PRINCIPLES.md` "Decline illegal
    /// operations").
    StageProposal,
    MarkAbandoned,
    NotifyUi,
}

/// Optional `mode:` argument on `dispatch`. Currently `intermediate` is the
/// only named mode; the default (no `mode:`) means a normal terminal
/// dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DispatchMode {
    Intermediate,
}

impl Action {
    /// Parse a workflow action from its YAML-string form.
    pub fn parse(src: &str) -> Result<Self, String> {
        let trimmed = src.trim();
        let (name, args) = split_call(trimmed)?;
        match name {
            "deliver_result" => no_args(name, &args).map(|_| Action::DeliverResult),
            // `compaction_merge` is the retired spelling (see
            // [`Action::LandCompaction`]) — parsed, never emitted.
            "land_compaction" | "compaction_merge" => {
                no_args(name, &args).map(|_| Action::LandCompaction)
            }
            "stage_proposal" => no_args(name, &args).map(|_| Action::StageProposal),
            "mark_abandoned" => no_args(name, &args).map(|_| Action::MarkAbandoned),
            "notify_ui" => no_args(name, &args).map(|_| Action::NotifyUi),
            "dispatch" => parse_dispatch(&args),
            "gate_return_on" => parse_gate_return_on(&args),
            other => Err(retired(other)
                .map(|why| format!("action {other:?} was retired: {why}; remove the binding"))
                .unwrap_or_else(|| format!("unknown action {other:?}"))),
        }
    }
}

/// Why a once-parsed action name is no longer vocabulary, or `None` if the
/// name was never in the closed set. Retired names are **declined** here
/// rather than accepted and silently ignored, so a config carrying stale
/// vocabulary fails at load with the reason (`docs/PRINCIPLES.md` "Decline
/// illegal operations"; the `manifest.yaml` `overflow: summarize`
/// subtraction is the same idiom).
fn retired(name: &str) -> Option<&'static str> {
    match name {
        // ARCH §2.4: "Reprompt is a message" — a user message resumes the
        // agent's own branch, and a *new* root agent is forked explicitly
        // off a config branch's head (§2.4, §3.4 CLI as control plane).
        // No binding ever spawns one, so the hop can never reach this.
        "spawn_root_agent" => Some(
            "a user message resumes the agent's own branch and a new root agent \
             is forked explicitly, so no binding spawns one (ARCH §2.4)",
        ),
        // ARCH §2.4: an exchange "is a UX span, not a structure … It owns
        // no branch, no merge, no lifecycle." There is nothing to spawn.
        "spawn_exchange" => Some(
            "an exchange is a UX span that owns no branch, merge, or lifecycle, \
             so there is nothing to spawn (ARCH §2.4)",
        ),
        _ => None,
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Arg {
    Positional(String),
    Named { key: String, value: String },
}

fn split_call(src: &str) -> Result<(&str, Vec<Arg>), String> {
    match src.find('(') {
        None => {
            validate_ident(src)?;
            Ok((src, Vec::new()))
        }
        Some(open) => {
            if !src.ends_with(')') {
                return Err(format!("missing closing ')' in {src:?}"));
            }
            let name = &src[..open];
            validate_ident(name)?;
            let inner = &src[open + 1..src.len() - 1];
            let args = parse_arg_list(inner)?;
            Ok((name, args))
        }
    }
}

fn parse_arg_list(inner: &str) -> Result<Vec<Arg>, String> {
    let inner = inner.trim();
    if inner.is_empty() {
        return Ok(Vec::new());
    }
    inner.split(',').map(|raw| parse_arg(raw.trim())).collect()
}

fn parse_arg(raw: &str) -> Result<Arg, String> {
    if raw.is_empty() {
        return Err("empty argument".into());
    }
    if let Some((k, v)) = raw.split_once(':') {
        let key = k.trim();
        let value = v.trim();
        validate_ident(key)?;
        validate_value(value)?;
        Ok(Arg::Named {
            key: key.into(),
            value: value.into(),
        })
    } else {
        validate_value(raw)?;
        Ok(Arg::Positional(raw.into()))
    }
}

fn validate_ident(s: &str) -> Result<(), String> {
    if s.is_empty() {
        return Err("empty identifier".into());
    }
    let ok = s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !ok {
        return Err(format!("not a valid identifier: {s:?}"));
    }
    Ok(())
}

fn validate_value(s: &str) -> Result<(), String> {
    if s.is_empty() {
        return Err("empty value".into());
    }
    let ok = s
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.');
    if !ok {
        return Err(format!("not a valid value: {s:?}"));
    }
    Ok(())
}

fn no_args(name: &str, args: &[Arg]) -> Result<(), String> {
    if !args.is_empty() {
        return Err(format!("{name} takes no arguments"));
    }
    Ok(())
}

fn parse_dispatch(args: &[Arg]) -> Result<Action, String> {
    let role = match args.first() {
        Some(Arg::Positional(role)) => role.clone(),
        _ => return Err("dispatch requires a positional role argument".into()),
    };
    let mut with = None;
    let mut mode = None;
    for arg in &args[1..] {
        match arg {
            Arg::Named { key, value } => match key.as_str() {
                "with" => with = Some(value.clone()),
                "mode" => mode = Some(parse_mode(value)?),
                other => return Err(format!("dispatch: unknown named arg {other:?}")),
            },
            Arg::Positional(_) => {
                return Err("dispatch takes at most one positional argument".into());
            }
        }
    }
    Ok(Action::Dispatch { role, with, mode })
}

fn parse_mode(value: &str) -> Result<DispatchMode, String> {
    match value {
        "intermediate" => Ok(DispatchMode::Intermediate),
        other => Err(format!("dispatch: unknown mode {other:?}")),
    }
}

fn parse_gate_return_on(args: &[Arg]) -> Result<Action, String> {
    match args {
        [Arg::Positional(predicate)] => Ok(Action::GateReturnOn {
            predicate: predicate.clone(),
        }),
        _ => Err("gate_return_on takes one positional predicate".into()),
    }
}

// Tests for the action DSL parser live in `tests/action_dsl.rs` so this
// file stays under the 300-line code-file limit.
