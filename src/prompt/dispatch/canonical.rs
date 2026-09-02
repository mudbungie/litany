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
/// default.
pub(super) fn build_request(
    model_id: &str,
    system: &str,
    messages: Vec<Message>,
    tools: Vec<Tool>,
    max_tokens: u32,
    effort: Option<Effort>,
) -> CanonicalRequest {
    CanonicalRequest {
        model: model_id.to_string(),
        system: Some(vec![Content::Text(system.to_string())]),
        messages,
        tools,
        max_tokens: Some(max_tokens),
        reasoning: effort.map(Into::into),
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
        );
        assert_eq!(req.model, "claude-sonnet-5");
        assert_eq!(req.max_tokens, Some(4096));
        assert_eq!(req.system, Some(vec![Content::Text("sys".into())]));
        assert_eq!(req.tools, vec![tool]);
        // `stream` absent → brazen default governs; `extra` stays empty.
        assert_eq!(req.stream, None);
        assert!(req.extra.is_empty());
        // No `effort:` on the assignment → the reasoning knob is unset
        // (§4.3 — the adapter's default, not a level of ours).
        assert_eq!(req.reasoning, None);
    }

    #[test]
    fn the_role_effort_rides_the_reasoning_knob() {
        // §4.3: the assignment's `effort:` is the whole source of the
        // request's `reasoning`; the level crosses unchanged.
        let req = build_request("m", "sys", vec![], vec![], 4096, Some(Effort::High));
        assert_eq!(req.reasoning, Some(brazen::ReasoningEffort::High));
    }
}
