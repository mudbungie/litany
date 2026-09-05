//! On-disk layout for a step (ARCH §2.3 and §2.10).
//!
//! Each step lives in its own directory under
//! `<conv-repo>/steps/<conv-id>/<NNN>/`, zero-padded 3-digit and
//! 1-indexed. The tree is at the conversation-repo root, *outside
//! every worktree* (§2.2 / §2.3), so context assembly (§3.5, §5)
//! cannot see step records as model context. Namespacing by
//! conversation id is what lets every conversation in the tree
//! (root and every subagent) write into a single shared `steps/`
//! tree without filename collision.
//!
//! Per-step files in v0.3.1+:
//!
//! - `meta.json` — `{commit, started_at, ended_at}`. The `commit`
//!   field is the sha of the branch tip at step-start; replay
//!   reproduces the wire input by re-running the context assembler
//!   (§5) against this commit's tree (§2.10).
//! - `request.json` — diagnostic snapshot of the wire request the
//!   model saw. Written for audit / human inspection only; the
//!   harness never reads it at runtime (§2.3 Diagnostic-only contract).
//! - `response.json` — JSONL of §4.4 stream events, appended by the
//!   harness as the adapter writes them. Writer-closes-fd is the
//!   `IN_CLOSE_WRITE` end-of-stream signal (§3.5). Diagnostic-only;
//!   the harness never reads it back (§2.3).
//! - `stderr.log` — the adapter subprocess's stderr, appended across
//!   the model call's attempts. Empty on an ordinary run: brazen
//!   speaks its failures in-band on stdout (§4.4), so bytes here mean
//!   the adapter failed *outside* that contract — a startup failure
//!   with nothing on stdout at all. Diagnostic-only; the tail quoted in
//!   a half-stream error comes from the live capture, never a read-back
//!   (§2.3).
//! - `tools/<tool-id>/` — per-tool-call records (`input.json`,
//!   `output.json`); diagnostic raw capture, written but never read at
//!   runtime (§2.3 Diagnostic-only contract). A tool result's runtime
//!   home is its transcript entry, `messages/NNN-tool.json` (§2.3, §3.3).
//! - `staging.json` — the transcript entry under construction (§2.3
//!   *The transcript writer*): the writer's own sink, not a diagnostic
//!   record, renamed out to the worktree at the model call's settling
//!   `Finish`.
//!
//! One file sits a level up, at `steps/<agent-id>/driver.log`: it belongs
//! to the agent's detached drivers rather than to any one step (§2.11).

use crate::provider::segment::{Outcome, classify};
use serde::{Deserialize, Serialize};

/// Top-level directory holding per-conversation step records, located
/// at the conversation-repo root outside every worktree (ARCH §2.2 /
/// §2.3). Joined onto the conv-repo path by writers, never the
/// worktree path.
pub const STEPS_DIR: &str = "steps";
/// Diagnostic snapshot of the wire request the model saw. Written
/// for audit only — harness never reads at runtime (§2.3).
pub const REQUEST_FILE: &str = "request.json";
/// JSONL of §4.4 stream events, written event-by-event by the harness
/// as the adapter emits them. End-of-stream is the writer closing the
/// fd (§3.5 IN_CLOSE_WRITE). Diagnostic-only; harness never reads it
/// back (§2.3).
pub const RESPONSE_FILE: &str = "response.json";
/// The adapter subprocess's stderr for a model call, appended per
/// attempt beside `response.json` (§2.3). Empty on an ordinary run —
/// brazen surfaces failures in-band on stdout (§4.4) — so a non-empty
/// file is the signature of an adapter that died outside that contract.
/// Diagnostic-only: written, never read back (§2.3).
pub const STDERR_FILE: &str = "stderr.log";
/// Step metadata: branch-tip sha at step-start plus timestamps
/// (§2.3). Readable by the harness — it carries the commit a
/// replay re-assembles against, which is the load-bearing piece.
pub const META_FILE: &str = "meta.json";
/// The model-output transcript entry *under construction* (ARCH §2.3
/// *The transcript writer*). Content blocks stream here block-by-block as
/// a JSON array; segment authority (§4.4) truncates it on an `Error`
/// segment, accumulates it on `Pause`, and the final `Finish` seals it,
/// whereupon the executor renames it into the worktree as
/// `messages/NNN-<model-id>.json` (§2.3). The one path under `steps/`
/// that is not a diagnostic record — the writer's own sink, never read
/// back as a step record (§2.3 Diagnostic-only contract).
pub const STAGING_FILE: &str = "staging.json";

/// A detached driver's stderr, at `steps/<agent-id>/driver.log` — one
/// level above the numbered step directories, because a driver spans the
/// steps it takes and a launch may take none at all (ARCH §2.11). The
/// same *capture, don't discard* rule the per-attempt [`STDERR_FILE`]
/// already obeys (§4.4 *Stderr is captured, not discarded*), applied to
/// the process whose stderr no operator is watching: a `setsid` driver
/// has no terminal to inherit, so its declines — a compaction landing
/// declined or superseded (§2.6), a launch that failed into the accepted
/// crash class (§2.11) — would otherwise be written to nothing. Those
/// declines are the operator notices of [`crate::prompt::notice`] and
/// each carries its prefix, which is how a program reading this file
/// tells them from a death rattle without matching prose (§2.11).
/// Append-only across launches and across the §6 exec baton (the
/// successor inherits the open fd), and diagnostic-only like every other
/// name in this tree: nothing reads it back. Non-numeric, so the step
/// derivations above ([`next_step_seq`], [`latest_step_outcome`]) skip it
/// exactly as they skip any other non-step entry.
pub const DRIVER_LOG_FILE: &str = "driver.log";

/// Width of the zero-padded step sequence in on-disk paths
/// (`steps/<conv-id>/001`, `…/002`, ...). Three digits gives comfortable
/// headroom for any realistic conversation while keeping directories
/// lexically sortable.
const STEP_SEQ_WIDTH: usize = 3;

/// The conv-repo-relative directory for step `seq` within conversation
/// `conv_id`. `seq` is 1-indexed. Joined onto the conv-repo root
/// (not any worktree) — step records live outside every worktree
/// per ARCH §2.2 / §2.3.
pub fn step_dir_rel(conv_id: &str, seq: u32) -> String {
    format!(
        "{STEPS_DIR}/{conv_id}/{seq:0width$}",
        width = STEP_SEQ_WIDTH
    )
}

/// The branch's next step sequence, derived — never stored — as
/// max-present-plus-one over the `steps/<conv-id>/` directory listing
/// (ARCH §6: workflow position is a function of disk state; the same
/// derivation discipline as the transcript counter, §2.3). An absent or
/// empty directory yields `1` — the general path with empty inputs, not
/// a bootstrap special case. A fresh `litany advance` hop reads its
/// position here instead of carrying a loop counter across the exec
/// baton.
pub fn next_step_seq(conv_repo: &std::path::Path, conv_id: &str) -> std::io::Result<u32> {
    let dir = conv_repo.join(STEPS_DIR).join(conv_id);
    let entries = match std::fs::read_dir(&dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(1),
        Err(e) => return Err(e),
    };
    let mut max = 0u32;
    for entry in entries {
        if let Ok(seq) = entry?.file_name().to_string_lossy().parse::<u32>() {
            max = max.max(seq);
        }
    }
    Ok(max + 1)
}

/// The framing outcome of `agent`'s latest step's `response.json`, or
/// `None` when no step tree, no numeric step, or no readable response
/// exists (the general path with empty inputs). Reads only the §4.4
/// framing tail via [`classify`] — a sanctioned framing read under the
/// §2.3 diagnostic-only contract (framing-yes / content-no).
///
/// This is the single derivation behind every "did this branch's work
/// end well?" question — the §8 silent-death sweep and the
/// `litany message` failed-branch advisory alike: a latest step that
/// never settled complete (§2.3) — [`Outcome::NoTerminal`] (killed or
/// stopped mid-work, §2.9) or [`Outcome::Failed`] (retries exhausted or
/// a non-retryable error, §2.10) — committed no transcript entry, so
/// the branch cannot advance without a new touch.
pub fn latest_step_outcome(workspace: &std::path::Path, agent: &str) -> Option<Outcome> {
    let bytes = std::fs::read(in_flight(workspace, agent)?.join(RESPONSE_FILE)).ok()?;
    Some(classify(&bytes))
}

/// The agent's **in-flight step** directory — the highest-numbered
/// `steps/<agent-id>/<NNN>/`, derived and never stored (the same
/// discipline as [`next_step_seq`], which is this plus one; ARCH §6:
/// position is a function of disk state). `None` when the agent has no
/// step tree or no numeric step in it — the general path with empty
/// inputs.
///
/// Two readers: the §8 silent-death sweep asks what the latest step's
/// framing was, and `litany invoke` asks which step's `tools/` an inner
/// invocation records under (`docs/DESIGN_CODE_EXECUTION.md` §2.3).
pub fn in_flight(workspace: &std::path::Path, agent: &str) -> Option<std::path::PathBuf> {
    std::fs::read_dir(workspace.join(STEPS_DIR).join(agent))
        .ok()?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.parse::<u32>().ok().map(|n| (n, e.path()))
        })
        .max_by_key(|(n, _)| *n)
        .map(|(_, p)| p)
}

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
/// Both are `Option` for exactly one reason: a `meta.json` written
/// before bl-e4a0 carries neither, and `None` says so. Every record
/// this harness writes carries both. Diagnostic provenance, the same
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
    pub started_at: String,
    pub ended_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_dir_rel_zero_pads_seq() {
        assert_eq!(
            step_dir_rel("20260422T000000Z-deadbeef", 1),
            "steps/20260422T000000Z-deadbeef/001"
        );
        assert_eq!(step_dir_rel("id", 42), "steps/id/042");
    }

    #[test]
    fn next_step_seq_is_one_for_a_fresh_branch() {
        let tmp = tempfile::TempDir::new().unwrap();
        assert_eq!(next_step_seq(tmp.path(), "c1").unwrap(), 1);
    }

    #[test]
    fn next_step_seq_is_max_present_plus_one_ignoring_junk() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path().join(STEPS_DIR).join("c1");
        std::fs::create_dir_all(dir.join("001")).unwrap();
        std::fs::create_dir_all(dir.join("007")).unwrap();
        std::fs::create_dir_all(dir.join("not-a-seq")).unwrap();
        assert_eq!(next_step_seq(tmp.path(), "c1").unwrap(), 8);
    }

    #[test]
    fn next_step_seq_surfaces_a_non_missing_read_error() {
        // A file where the step directory should be is a real error,
        // not the general empty case.
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(STEPS_DIR)).unwrap();
        std::fs::write(tmp.path().join(STEPS_DIR).join("c1"), b"x").unwrap();
        assert!(next_step_seq(tmp.path(), "c1").is_err());
    }

    #[test]
    fn step_meta_round_trips_and_publishes_stable_keys() {
        let m = StepMeta {
            commit: "0123456789abcdef0123456789abcdef01234567".into(),
            config_commit: Some("cfg1111111111111111111111111111111111111".into()),
            workflow_commit: Some("wf22222222222222222222222222222222222222".into()),
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
            "started_at",
            "ended_at",
        ] {
            assert!(v.get(key).is_some(), "missing key: {key}");
        }
    }

    #[test]
    fn a_record_written_before_the_provenance_fields_still_reads() {
        // Grows-only serde (bl-e4a0): every `meta.json` on every box
        // predating the two config shas must keep parsing, and the
        // absence must read as "not recorded" rather than as a sha.
        // `budget::derive` sums wall-clock off these records and would
        // otherwise start scoring every historical step as zero.
        let back: StepMeta = serde_json::from_str(
            r#"{"commit":"abc","started_at":"2026-04-22T06:54:32Z","ended_at":"2026-04-22T06:54:35Z"}"#,
        )
        .unwrap();
        assert_eq!(back.config_commit, None);
        assert_eq!(back.workflow_commit, None);
    }
}
