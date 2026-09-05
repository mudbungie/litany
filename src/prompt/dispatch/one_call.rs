//! **One model call, issued and recorded** — the step's middle, shared
//! by the two drivers (ARCH §2.3, §2.9 step 3, §2.10).
//!
//! `run_exchange`'s loop body and the `litany advance` hop had this
//! sequence spelled out twice, byte for byte apart from how each exits
//! on a stop: snapshot the request, call the adapter, classify a stop,
//! then record the step's `meta.json`. It is the one stretch of the step
//! with no per-driver decision in it — every difference is at the ends,
//! in what each assembles beforehand and what each does with the
//! response — so it has one home, and the pair whose only divergence was
//! `break` versus `return` now share the answer as a value.
//!
//! A stop answers `None`. The flag classifies, not the error's shape
//! ([`super::model_call`]): a stop during the call killed `bz`, so
//! whatever it surfaced is swallowed with the on-disk signature
//! untouched, and each caller finishes on its own terminal path (§2.9).

use super::step_commit::{write_meta, write_request};
use super::{Resolved, model_call, stop_signal};
use crate::prompt::step::{RESPONSE_FILE, StepMeta, step_dir_rel};
use crate::prompt::{Deps, Error};
use brazen::CanonicalRequest;
use std::path::Path;

/// Where one step's record goes and what read state it names — the
/// four facts that travel together because none of them means anything
/// without the others: the conv-repo the `steps/` tree lives under
/// (§2.3), the conversation namespacing it, the sequence within that
/// conversation, and the branch tip captured at step-start that
/// `meta.json` records as the read state (§2.10).
pub(super) struct Step<'a> {
    pub(super) conv_repo: &'a Path,
    pub(super) conv_id: &'a str,
    pub(super) seq: u32,
    pub(super) tip: String,
}

/// Issue `request` for `step`, recording `request.json`, the response
/// stream and `meta.json` under `<conv-repo>/steps/` (§2.3). Answers the
/// step's conv-repo-relative directory, or `None` when a stop felled the
/// call.
///
/// `resolved` supplies the step's policy provenance beside the read
/// state (bl-e4a0).
pub(super) fn issue(
    step: Step<'_>,
    request: &CanonicalRequest,
    call: &model_call::ModelCall<'_>,
    resolved: &Resolved<'_>,
    deps: &Deps<'_>,
) -> Result<Option<String>, Error> {
    let request_value =
        serde_json::to_value(request).expect("CanonicalRequest is always serializable");
    let step_dir_rel_str = step_dir_rel(step.conv_id, step.seq);
    write_request(step.conv_repo, &step_dir_rel_str, &request_value)?;

    let request_bytes =
        serde_json::to_vec(request).expect("CanonicalRequest is always serializable");
    let started_at = deps.clock.now_iso8601();
    let response_path = step.conv_repo.join(&step_dir_rel_str).join(RESPONSE_FILE);
    let call_outcome = model_call::run(call, &request_bytes, &response_path);
    if stop_signal::stopped(deps.stop) {
        return Ok(None);
    }
    call_outcome?;
    let ended_at = deps.clock.now_iso8601();

    write_meta(
        step.conv_repo,
        &step_dir_rel_str,
        &StepMeta {
            commit: step.tip,
            config_commit: Some(resolved.grant.config_commit.to_string()),
            workflow_commit: Some(resolved.workflow_commit.to_string()),
            started_at,
            ended_at,
        },
    )?;
    Ok(Some(step_dir_rel_str))
}
