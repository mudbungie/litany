//! Which event a **returning child** names (ARCH §6 *Circumstance is
//! derived from disk*; `docs/DESIGN_PROCEDURES.md` §5) — one derivation,
//! in order, for a child of role R whose leading word is T:
//!
//! 1. **Gating child.** If R's return is the gating child of a held
//!    result (ancestry: it was forked off that result's terminal ref), the
//!    gate's predicate `gate_return_on(R.P)` bound on the held result's
//!    return event decides: epitaph `final-response` and T = P ⇒ `R_P`
//!    (met); anything else ⇒ `R_reject` (unmet).
//! 2. **Bound token.** Else if the epitaph is `final-response` and the
//!    workflow binds `R_T`: `R_T`.
//! 3. **Return.** Else `R_return`.
//!
//! The guards the ruling asked for are structural here: R is read from
//! the dispatch commit the harness wrote (never from the child's text);
//! the result is interpreted only in its dispatcher's inbox
//! ([`super::own_result_ref`]); a token nobody bound is no event; and a
//! gating child is found by ancestry, so a verifier the model dispatched
//! by hand gates nothing.

use super::{ChildResult, verifier};
use crate::config::{Action, Event, Workflow};
use crate::prompt::inbox::Epitaph;
use crate::prompt::{Error, WORKER_ROLE, role};
use crate::template::GitRunner;
use std::path::Path;

/// A returning child's circumstance: its event, and — when it is the
/// gating child of a held result — that result's index among the pending
/// results and its role.
pub(super) struct Circumstance {
    pub(super) event: Event,
    pub(super) gated: Option<Gated>,
}

/// The held result a gating child answers for.
pub(super) struct Gated {
    pub(super) index: usize,
    pub(super) role: String,
}

/// Derive `cr`'s circumstance among `results` under `workflow` (module
/// docs).
pub(super) fn derive(
    worktree: &Path,
    cr: &ChildResult,
    results: &[ChildResult],
    workflow: &Workflow,
    git: &dyn GitRunner,
) -> Result<Circumstance, Error> {
    let role = role_of(worktree, cr, git)?;
    let finished = cr.epitaph == Epitaph::FinalResponse.as_str();
    let word = cr.response.as_deref().and_then(leading_word);
    if let Some(index) = verifier::gated_index(results, cr, worktree, git) {
        let gated_role = role_of(worktree, &results[index], git)?;
        if let Some(token) = gate_token(workflow, &gated_role, &role) {
            let met = finished && word.as_deref() == Some(token.as_str());
            return Ok(Circumstance {
                event: if met {
                    Event::verdict(&role, &token)
                } else {
                    Event::rejected(&role)
                },
                gated: Some(Gated {
                    index,
                    role: gated_role,
                }),
            });
        }
    }
    if finished
        && let Some(token) = word
        && workflow.events.contains_key(&Event::verdict(&role, &token))
    {
        return Ok(Circumstance {
            event: Event::verdict(&role, &token),
            gated: None,
        });
    }
    Ok(Circumstance {
        event: Event::returned(&role),
        gated: None,
    })
}

/// The returning child's role, from its dispatch commit subject (ARCH
/// §2.5, the single authoritative home); a root founded before bl-946c
/// has none and is the worker.
fn role_of(worktree: &Path, cr: &ChildResult, git: &dyn GitRunner) -> Result<String, Error> {
    Ok(role::derive(worktree, &cr.terminal_ref, &cr.child_id, git)?
        .unwrap_or_else(|| WORKER_ROLE.to_string()))
}

/// The first word of a response's first non-empty line, lowercased —
/// the token a verdict event is matched on. Letters and digits only, so
/// `APPROVE`, `approve` and `APPROVE:` are one word.
pub(super) fn leading_word(response: &str) -> Option<String> {
    let line = response.lines().map(str::trim).find(|l| !l.is_empty())?;
    let word: String = line
        .chars()
        .take_while(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase();
    (!word.is_empty()).then_some(word)
}

/// The token a gating child of `role` must say to lift the hold on a
/// result of `gated_role`: the `gate_return_on(<role>.<token>)` bound on
/// `<gated_role>_return`, if one names this role.
fn gate_token(workflow: &Workflow, gated_role: &str, role: &str) -> Option<String> {
    workflow
        .actions_for(&Event::returned(gated_role))
        .into_iter()
        .find_map(|action| match action {
            Action::GateReturnOn { predicate } => predicate
                .split_once('.')
                .filter(|(r, _)| *r == role)
                .map(|(_, token)| token.to_string()),
            _ => None,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_leading_word_is_the_first_word_of_the_first_line_lowercased() {
        assert_eq!(leading_word("APPROVE").as_deref(), Some("approve"));
        assert_eq!(
            leading_word("\n  REJECT: bad\nmore").as_deref(),
            Some("reject")
        );
        assert_eq!(leading_word("Switch now").as_deref(), Some("switch"));
        assert_eq!(leading_word("  \n\n"), None);
        assert_eq!(leading_word("- not a word"), None);
    }

    #[test]
    fn the_gate_token_is_read_off_the_held_results_return_binding() {
        let wf = Workflow::parse(
            "events:\n  worker_return:\n    - dispatch(verifier)\n    - gate_return_on(verifier.approve)\n",
            Path::new("w"),
        )
        .unwrap();
        assert_eq!(
            gate_token(&wf, "worker", "verifier").as_deref(),
            Some("approve")
        );
        // Another role's return is not this gate; another role is not
        // its gating child.
        assert_eq!(gate_token(&wf, "verifier", "verifier"), None);
        assert_eq!(gate_token(&wf, "worker", "reviewer"), None);
    }
}
