//! The typed canonical request litany composes (ARCH §4.4 "the
//! vocabulary is linked") — request composition, held apart from
//! [`super::model_call`], which owns execution: the attempt loop, the
//! retry classification, and the stream framing. One builds what every
//! attempt sends; the other decides how many attempts send it.

use crate::config::Effort;
use brazen::{CanonicalRequest, Content, Message, Tool};

/// Build a typed [`CanonicalRequest`] (§4.4): building the struct
/// directly makes brazen's fail-open `extra` map unreachable. `stream`
/// is left `None` — streaming is brazen's default and litany never
/// overrides it (§4.4). `tools` carries the role's composed toolset
/// (§3.3 — the schemas the model is told it may call); an empty vec is
/// "no tools declared/available". `effort` is the role's assignment
/// level (§4.3): it rides the request's `reasoning` knob — the lifted
/// canonical knob each protocol projects to its native reasoning shape
/// inside brazen — and `None` leaves the knob unset, the adapter's own
/// default. `priority` is the assignment's lane request (§4.3): true
/// rides the `service_tier` knob as [`brazen::ServiceTier::Priority`],
/// the lifted processing-lane intent each protocol projects to its own
/// spelling; false and absent are one fact and leave the knob unset, so
/// the provider's default lane governs. litany never asks for
/// `Standard` — refusing the priority lane outright is a different
/// intent from having no preference, and no config key expresses it.
pub(super) fn build_request(
    model_id: &str,
    system: &str,
    messages: Vec<Message>,
    tools: Vec<Tool>,
    max_tokens: u32,
    effort: Option<Effort>,
    priority: Option<bool>,
) -> CanonicalRequest {
    CanonicalRequest {
        model: model_id.to_string(),
        system: Some(vec![Content::Text(system.to_string())]),
        messages,
        tools,
        max_tokens: Some(max_tokens),
        reasoning: effort.map(Into::into),
        service_tier: priority
            .unwrap_or(false)
            .then_some(brazen::ServiceTier::Priority),
        ..CanonicalRequest::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_request_is_a_typed_canonical_request() {
        // Message pass-through is asserted in the e2e test; here we pin
        // the typed shape and the composed `tools` array (§3.3).
        let tool = brazen::Tool::Custom {
            name: "bash".into(),
            description: None,
            input_schema: serde_json::json!({"type": "object"}),
            strict: None,
        };
        let req = build_request(
            "claude-sonnet-5",
            "sys",
            vec![],
            vec![tool.clone()],
            4096,
            None,
            None,
        );
        assert_eq!(req.model, "claude-sonnet-5");
        assert_eq!(req.max_tokens, Some(4096));
        assert_eq!(req.system, Some(vec![Content::Text("sys".into())]));
        assert_eq!(req.tools, vec![tool]);
        // `stream` absent → brazen default governs; `extra` stays empty.
        assert_eq!(req.stream, None);
        assert!(req.extra.is_empty());
        // No `effort:` on the assignment → the reasoning knob is unset
        // (§4.3 — the adapter's default, not a level of ours), and no
        // `priority:` leaves the lane knob absent the same way.
        assert_eq!(req.reasoning, None);
        assert_eq!(req.service_tier, None);
    }

    #[test]
    fn the_role_effort_rides_the_reasoning_knob() {
        // §4.3: the assignment's `effort:` is the whole source of the
        // request's `reasoning`; the level crosses unchanged.
        let req = build_request("m", "sys", vec![], vec![], 4096, Some(Effort::High), None);
        assert_eq!(req.reasoning, Some(brazen::ReasoningEffort::High));
    }

    #[test]
    fn the_role_priority_rides_the_service_tier_knob() {
        // §4.3: `priority: true` asks the provider's priority lane, as
        // the canonical processing-lane intent — brazen projects it per
        // dialect (OpenAI `"priority"`, Anthropic's asymmetric `"auto"`,
        // which is org-provisioned capacity with a standard fallback).
        let req = build_request("m", "sys", vec![], vec![], 4096, None, Some(true));
        assert_eq!(req.service_tier, Some(brazen::ServiceTier::Priority));
    }

    #[test]
    fn an_explicit_false_priority_is_the_same_absence_as_none() {
        // The checkbox has two states, not three (§4.3): unchecked is
        // "no lane preference", byte-for-byte the omitted case — so no
        // config can make litany demand the standard lane by accident.
        let unchecked = build_request("m", "sys", vec![], vec![], 4096, None, Some(false));
        let absent = build_request("m", "sys", vec![], vec![], 4096, None, None);
        assert_eq!(unchecked.service_tier, None);
        assert_eq!(unchecked, absent);
    }
}
