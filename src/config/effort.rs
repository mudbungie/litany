//! The role's **effort** level (ARCH §4.3) — how much reasoning the
//! role's model calls request.
//!
//! The config vocabulary is `low | medium | high`, the same canonical
//! set the pinned adapter lifts (brazen `specs/providers.md` §6: one
//! canonical knob, per-dialect wire shapes — OpenAI `reasoning_effort`,
//! Anthropic `thinking.budget_tokens`, and siblings). "Effort" is the
//! config's one word for the fact; the provider spellings never leave
//! the adapter. The enum is litany's own rather than a re-export of
//! `brazen::ReasoningEffort` because the config schema surface
//! ([`crate::config::schemas`]) needs `JsonSchema`, which the linked
//! crate's type does not carry — the [`From`] impl below is the whole
//! bridge, one match, converted at the request-build seam.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A role's requested reasoning-effort level (ARCH §4.3 `effort:`).
/// Absent (`Option::None` on the assignment) means no reasoning is
/// requested — the canonical request's `reasoning` stays unset, the
/// adapter's own default behavior.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    Low,
    Medium,
    High,
}

impl From<Effort> for brazen::ReasoningEffort {
    fn from(effort: Effort) -> Self {
        match effort {
            Effort::Low => brazen::ReasoningEffort::Low,
            Effort::Medium => brazen::ReasoningEffort::Medium,
            Effort::High => brazen::ReasoningEffort::High,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_speaks_lowercase() {
        // The config vocabulary is the canonical lowercase set; the
        // round trip is byte-stable.
        for (level, word) in [
            (Effort::Low, "low"),
            (Effort::Medium, "medium"),
            (Effort::High, "high"),
        ] {
            assert_eq!(serde_yaml_ng::to_string(&level).unwrap().trim(), word);
            let back: Effort = serde_yaml_ng::from_str(word).unwrap();
            assert_eq!(back, level);
        }
    }

    #[test]
    fn converts_level_for_level_to_the_adapter_vocabulary() {
        // The bridge to the linked crate is level-for-level — no
        // remapping, ever: the two vocabularies are one set.
        assert_eq!(
            brazen::ReasoningEffort::from(Effort::Low),
            brazen::ReasoningEffort::Low
        );
        assert_eq!(
            brazen::ReasoningEffort::from(Effort::Medium),
            brazen::ReasoningEffort::Medium
        );
        assert_eq!(
            brazen::ReasoningEffort::from(Effort::High),
            brazen::ReasoningEffort::High
        );
    }
}
