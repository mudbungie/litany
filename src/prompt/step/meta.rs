//! `meta.json` — the step record's own metadata (ARCH §2.3).
//!
//! Split out of [`super`] (the step's on-disk *layout*: names, paths and
//! the derivations over them) because this is the record's *shape*: one
//! serde struct, its provenance rationale, and the grows-only round-trip
//! that keeps every older record readable.

use serde::{Deserialize, Serialize};

/// On-disk shape of `meta.json`. The `commit` field is the branch
/// tip's sha at step-start — the read state for the model call
/// (§2.10). `started_at` / `ended_at` bookend the call's wall-clock
/// duration. Replay tooling reads `commit` to locate the tree state
/// the request was assembled against.
///
/// **The two config shas are the step's policy provenance** (bl-e4a0,
/// `docs/DESIGN_CONFIG_FOLLOW.md` §1). Under follow-the-tip a
/// conversation resolves the workspace's *current* config at every step
/// boundary, so "which config governed step N" stopped being derivable
/// from the branch's ancestry and became a fact about **when** the step
/// ran — knowable only if the step records it. `config_commit` is the
/// commit this step resolved all control from ([`crate::workspace::current_config`]);
/// `workflow_commit` is the commit its `workflow.yaml` came from — the
/// same sha for every unmarked agent, and the workflow mark's commit
/// when one stood (§6). Both are written whole rather than one being
/// conditional on the other, so a reader that finds them equal knows no
/// mark stood, rather than having to tell "no mark" from "not recorded".
///
/// **`provider` is the row the call was billed through** (bl-4c1c). The
/// model id is already on disk in the step's `request.json`, but a model
/// id does not price a step on its own: the same id costs nothing
/// marginal through a subscription row and list price through an
/// API-key row, so what a step's tokens cost is a fact about the pair
/// `(row, model)`. The row is knowable only here — it is the `provider:`
/// the role resolved from the config commit and the exact string the
/// harness hands `bz --provider` (§4.4) — so the step records it. Like
/// the two shas above it is *recorded, never computed* (§2.3): the fact
/// the adapter was invoked with, not a later re-resolution of a config
/// that may since have advanced. litany learns no rate from it, sums
/// nothing new, and the §6 budget derivation is untouched — the field is
/// provenance a reader prices, and pricing is not litany's question.
///
/// **`dropped_orphans` is what assembly refused to send** (bl-2d93). An
/// orphan `tool_result` — a result whose `tool_use` a cut took out of
/// context — is refused by every provider on every later prompt, so
/// assembly drops the block rather than composing a history the branch
/// can never get past ([`crate::prompt::dispatch::pairing`]). Dropping
/// it silently would leave the wire disagreeing with the record with
/// nothing saying so, so the ids land here, in the step that first sent
/// the repaired history. Empty for every step of every branch no cut has
/// split — a `Vec` rather than an `Option` because "none dropped" and
/// "not recorded" are the same fact for a field whose absence a reader
/// can only read as empty.
///
/// The first three are `Option` for exactly one reason: a `meta.json` written
/// before the field existed carries none, and `None` says so. Every record
/// this harness writes carries all three. Diagnostic provenance, the same
/// class as `request.json` — read by audit and by a human, never a
/// control input the harness feeds back (§2.3 *Diagnostic-only
/// contract*); `commit` remains the one field replay is premised on.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StepMeta {
    pub commit: String,
    /// The **followed config commit** this step resolved control from
    /// (§2.2, bl-403b). `None` only in a record written before the field
    /// existed.
    #[serde(default)]
    pub config_commit: Option<String>,
    /// The commit whose `workflow.yaml` this step ran (§6): the nearest
    /// standing workflow mark's, else `config_commit`. `None` only in a
    /// record written before the field existed.
    #[serde(default)]
    pub workflow_commit: Option<String>,
    /// The **provider row** the step's model call was issued through
    /// (bl-4c1c): verbatim the string handed to `bz --provider`. `None`
    /// only in a record written before the field existed — every model
    /// call names a row, so the harness always writes one.
    #[serde(default)]
    pub provider: Option<String>,
    /// The `tool_use` ids of the orphan `tool_result` blocks assembly
    /// dropped from this step's wire history (bl-2d93). Empty is the
    /// standing case; a record written before the field existed reads
    /// as empty too.
    #[serde(default)]
    pub dropped_orphans: Vec<String>,
    pub started_at: String,
    pub ended_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_meta_round_trips_and_publishes_stable_keys() {
        let m = StepMeta {
            commit: "0123456789abcdef0123456789abcdef01234567".into(),
            config_commit: Some("cfg1111111111111111111111111111111111111".into()),
            workflow_commit: Some("wf22222222222222222222222222222222222222".into()),
            provider: Some("anthropic".into()),
            dropped_orphans: vec!["call_gone".into()],
            started_at: "2026-04-22T06:54:32Z".into(),
            ended_at: "2026-04-22T06:54:35Z".into(),
        };
        let json = serde_json::to_string(&m).unwrap();
        let back: StepMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        for key in [
            "commit",
            "config_commit",
            "workflow_commit",
            "provider",
            "dropped_orphans",
            "started_at",
            "ended_at",
        ] {
            assert!(v.get(key).is_some(), "missing key: {key}");
        }
    }

    #[test]
    fn a_record_written_before_the_provenance_fields_still_reads() {
        // Grows-only serde (bl-e4a0, bl-4c1c): every `meta.json` on every
        // box predating the two config shas and the provider row must keep
        // parsing, and the absence must read as "not recorded" rather than
        // as a sha or a row. `budget::derive` sums wall-clock off these
        // records and would otherwise start scoring every historical step
        // as zero.
        let back: StepMeta = serde_json::from_str(
            r#"{"commit":"abc","started_at":"2026-04-22T06:54:32Z","ended_at":"2026-04-22T06:54:35Z"}"#,
        )
        .unwrap();
        assert_eq!(back.config_commit, None);
        assert_eq!(back.workflow_commit, None);
        assert_eq!(back.provider, None);
        assert!(back.dropped_orphans.is_empty());
    }
}
