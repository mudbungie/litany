//! Workflow **events** (ARCH §6) — the circumstances a hop derives from
//! disk and matches against the flat binding list.
//!
//! Six are **closed**: the circumstances a branch's own hop fires —
//! `user_message`, `worker_flush`, `branch_stopped`, and the three
//! per-step hooks. The rest are **one open axis**, a returning child's
//! (`docs/DESIGN_PROCEDURES.md` §5), keyed on the child's **role** — the
//! harness's own fact, read from the dispatch commit, never from the
//! child's text:
//!
//! - **`<role>_return`** — a child of `<role>` returned. `return` is a
//!   reserved suffix. `worker_return`, `compactor_return` and
//!   `reviewer_return` are this shape.
//! - **`<role>_<token>`** — a child of `<role>` returned on
//!   `final-response` with a leading line whose first word is `<token>`
//!   (case-insensitively), *and the workflow binds this event*. A token
//!   nobody bound is no event: the return is `<role>_return`.
//!   `verifier_approve` is this shape.
//! - **`<role>_reject`** — the **gate's unmet outcome** for a gating
//!   child of `<role>`: it returned without satisfying the predicate
//!   `gate_return_on(<role>.<token>)`, whatever it said and however it
//!   ended. `reject` is the second reserved suffix; it names the gate's
//!   outcome, not a word the child said — which is what `verifier_reject`
//!   has always meant ("every other finished outcome rejects").
//!
//! The split is at the **last** `_`, so a role name may carry underscores
//! and a token may not; a closed name wins over the shape (so `branch_stopped`
//! is never a `branch` role's `stopped` verdict). Adding a closed event is
//! a code change; a return or verdict event is config.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The reserved suffix naming a child's plain return.
const RETURN: &str = "return";
/// The reserved suffix naming the gate's unmet outcome.
const REJECT: &str = "reject";

/// One workflow event (module docs).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Event {
    UserMessage,
    WorkerFlush,
    BranchStopped,
    PreStep,
    PostStep,
    OnToolReturn,
    /// `<role>_return`.
    Return(String),
    /// `<role>_<token>`, bound by the workflow and said by the child.
    Verdict {
        role: String,
        token: String,
    },
    /// `<role>_reject`: the gate's unmet outcome.
    Reject(String),
}

impl Event {
    /// A child of `role` returned.
    pub fn returned(role: &str) -> Self {
        Event::Return(role.to_string())
    }

    /// A child of `role` said `token` (module docs).
    pub fn verdict(role: &str, token: &str) -> Self {
        Event::Verdict {
            role: role.to_string(),
            token: token.to_string(),
        }
    }

    /// A gating child of `role` returned unmet.
    pub fn rejected(role: &str) -> Self {
        Event::Reject(role.to_string())
    }

    /// Parse a `workflow.yaml` event key (module docs).
    pub fn parse(key: &str) -> Result<Self, String> {
        let closed = match key {
            "user_message" => Some(Event::UserMessage),
            "worker_flush" => Some(Event::WorkerFlush),
            "branch_stopped" => Some(Event::BranchStopped),
            "pre_step" => Some(Event::PreStep),
            "post_step" => Some(Event::PostStep),
            "on_tool_return" => Some(Event::OnToolReturn),
            _ => None,
        };
        if let Some(event) = closed {
            return Ok(event);
        }
        let Some((role, suffix)) = key.rsplit_once('_') else {
            return Err(format!(
                "unknown event {key:?}: not a lifecycle event, and not of the \
                 `<role>_return` / `<role>_<token>` shape"
            ));
        };
        if role.is_empty()
            || !role
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            return Err(format!("event {key:?}: {role:?} is not a role name"));
        }
        if suffix.is_empty()
            || !suffix
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        {
            return Err(format!(
                "event {key:?}: {suffix:?} is not a verdict token (lowercase letters and digits)"
            ));
        }
        Ok(match suffix {
            RETURN => Event::Return(role.to_string()),
            REJECT => Event::Reject(role.to_string()),
            _ => Event::Verdict {
                role: role.to_string(),
                token: suffix.to_string(),
            },
        })
    }

    /// The `workflow.yaml` key for this event (ARCH §6) — the stable name
    /// used in diagnostics and by the runtime interpreter; the inverse of
    /// [`Event::parse`].
    pub fn key(&self) -> String {
        match self {
            Event::UserMessage => "user_message".into(),
            Event::WorkerFlush => "worker_flush".into(),
            Event::BranchStopped => "branch_stopped".into(),
            Event::PreStep => "pre_step".into(),
            Event::PostStep => "post_step".into(),
            Event::OnToolReturn => "on_tool_return".into(),
            Event::Return(role) => format!("{role}_{RETURN}"),
            Event::Reject(role) => format!("{role}_{REJECT}"),
            Event::Verdict { role, token } => format!("{role}_{token}"),
        }
    }
}

impl Serialize for Event {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.key())
    }
}

impl<'de> Deserialize<'de> for Event {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let key = String::deserialize(deserializer)?;
        Event::parse(&key).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_closed_six_parse_and_round_trip() {
        for key in [
            "user_message",
            "worker_flush",
            "branch_stopped",
            "pre_step",
            "post_step",
            "on_tool_return",
        ] {
            let event = Event::parse(key).unwrap();
            assert!(!matches!(
                event,
                Event::Return(_) | Event::Verdict { .. } | Event::Reject(_)
            ));
            assert_eq!(event.key(), key, "{key}");
        }
    }

    #[test]
    fn the_open_axis_splits_at_the_last_underscore() {
        assert_eq!(
            Event::parse("worker_return").unwrap(),
            Event::returned("worker")
        );
        assert_eq!(
            Event::parse("verifier_approve").unwrap(),
            Event::verdict("verifier", "approve")
        );
        assert_eq!(
            Event::parse("verifier_reject").unwrap(),
            Event::rejected("verifier")
        );
        // A role may carry underscores; the token is what follows the last.
        assert_eq!(
            Event::parse("code_reviewer_return").unwrap(),
            Event::returned("code_reviewer")
        );
        assert_eq!(
            Event::parse("code_reviewer_lgtm").unwrap(),
            Event::verdict("code_reviewer", "lgtm")
        );
        for (event, key) in [
            (Event::returned("titler"), "titler_return"),
            (Event::verdict("guard", "injected"), "guard_injected"),
            (Event::rejected("verifier"), "verifier_reject"),
        ] {
            assert_eq!(event.key(), key);
            assert_eq!(Event::parse(key).unwrap(), event);
        }
    }

    #[test]
    fn a_key_outside_both_shapes_is_refused_with_the_reason() {
        let e = Event::parse("teleport").unwrap_err();
        assert!(e.contains("not a lifecycle event"), "{e}");
        let e = Event::parse("_return").unwrap_err();
        assert!(e.contains("not a role name"), "{e}");
        let e = Event::parse("Worker_return").unwrap_err();
        assert!(e.contains("not a role name"), "{e}");
        let e = Event::parse("worker_").unwrap_err();
        assert!(e.contains("not a verdict token"), "{e}");
        let e = Event::parse("worker_APPROVE").unwrap_err();
        assert!(e.contains("not a verdict token"), "{e}");
    }

    #[test]
    fn serde_reads_and_writes_the_key() {
        let event: Event = serde_yaml_ng::from_str("titler_return").unwrap();
        assert_eq!(event, Event::returned("titler"));
        assert_eq!(
            serde_yaml_ng::to_string(&event).unwrap().trim(),
            "titler_return"
        );
        assert!(serde_yaml_ng::from_str::<Event>("teleport").is_err());
    }
}
