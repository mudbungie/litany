//! **Host injection**: the binding's own tools, declared and answered
//! (ARCH §3.3 *Host-injected tools*, §3.4; `docs/DESIGN_TOOL_INJECTION.md`).
//!
//! The exec binding needs none of this. A *linked* binding may need two
//! things the pool cannot give it: to put tool definitions of its own in
//! front of the model, and to answer some invocations itself rather than
//! have the executor resolve a binary for them — a client-management
//! tool, or a tool a remote client advertises across a transport only the
//! host speaks (yog's client/server split, its `docs/REMOTE.md` §5).
//!
//! Both halves ride **one** object the binding injects at
//! `cmd::Fx::tool_injection`, and one object is the point: a declaration
//! half without a permission half produces a tool the model is told about
//! and then refused ("declaring is not permitting", §3.3), and a
//! permission half without a declaration produces a tool nothing ever
//! calls. Held together they cannot disagree — [`ToolInjection::tools`]
//! is read by prompt assembly *and* by the grant gate, and
//! [`ToolInjection::route`] answers on the same object's behalf.
//!
//! What this seam deliberately is not:
//!
//! - **Not a multiplexer.** Each injected tool is individually named, so
//!   the grant gate, the fork-time descriptor trim and the tool control
//!   (§3.3) all keep seeing one name per capability. This is
//!   `docs/DESIGN_MCP_BRIDGE.md` §6's ruling, unchanged and now also
//!   binding on the host.
//! - **Not dynamic mid-drive.** The set is whatever the host states while
//!   the drive runs; a host that changes it changes the prompt prefix and
//!   pays the cache rebuild knowingly (ARCH §5.5).
//! - **Not an adjudication bypass.** A routed invocation is gated by the
//!   grant and adjudicated by the configured tool control exactly as a
//!   local one, *before* anything is routed (§3.3 *Tool control*).

use serde_json::Value;
use std::path::Path;
use std::sync::atomic::AtomicBool;

/// One tool definition spliced into a request by something other than the
/// calling role's `providers.yaml` `tools:` grant (ARCH §3.3) — the
/// compactor's procedure toolset (§2.7) and the host's injection are both
/// this shape, so the composer has one kind of injected thing to splice.
///
/// It carries exactly the three facts the `tools: [...]` entry needs; a
/// pool tool sources the same three from disk (`descriptions/tools/
/// <name>.json` plus the skill frontmatter), and an injected one has no
/// disk to source them from, which is the whole difference.
pub struct InjectedTool {
    /// The name the model spells in its `tool_use` block, and the name
    /// the grant gate and the router both key on.
    pub name: String,
    /// Sent verbatim as the entry's `input_schema`, exactly as a
    /// committed `descriptions/tools/<name>.json` is (§3.3).
    pub input_schema: Value,
    /// The entry's `description` — what a pool tool takes from its
    /// `SKILL.md` frontmatter. `None` composes an entry without one.
    pub description: Option<String>,
}

/// One invocation handed to a host router. The four wire facts a tool
/// subprocess gets on stdin and in its environment (§3.3 *Stdio
/// contract*), plus the cancel flag — nothing else, because a router
/// that needed more would be reaching for harness state the front door
/// does not carry.
pub struct RoutedCall<'a> {
    /// `tool_use.id` from the wire — the per-tool-call record's directory
    /// name (§3.3 *Disk record*), and the id a host correlates on.
    pub id: &'a str,
    /// The tool name as the model spelled it.
    pub name: &'a str,
    /// `tool_use.input`, verbatim — a subprocess's stdin.
    pub input: &'a Value,
    /// The calling agent's workspace root — the `LITANY_CONV_REPO` a
    /// subprocess reads from its environment.
    pub workspace: &'a Path,
    /// The calling agent's id (== branch name / hyphenated descent,
    /// §2.3) — the `LITANY_CONV_BRANCH` a subprocess reads.
    pub agent: &'a str,
    /// The §2.9 cancel flag, set when a stop lands mid-invocation. A
    /// router that blocks must watch it: it is the only thing that can
    /// tell an in-process router the drive is being torn down.
    pub stop: &'a AtomicBool,
}

/// What a router produced for one invocation — the same three facts a
/// tool subprocess produces (§3.3 *Stdio contract*), so everything
/// downstream is unchanged: the result envelope states the exit code,
/// `is_error` is `exit_code != 0`, the bounded projection caps both
/// streams, and `output.json` records them in full. A routed tool is
/// indistinguishable from a local one to the model, by construction
/// rather than by convention.
pub struct RoutedCapture {
    /// The tool's product. Carried verbatim into the result envelope.
    pub stdout: Vec<u8>,
    /// Diagnostics. Carried under the envelope's `--- stderr ---` marker
    /// whenever non-empty, success included.
    pub stderr: Vec<u8>,
    /// 0 for success, non-zero for an in-band failure. This is where a
    /// vanished remote endpoint lands: it is a failed invocation the
    /// model reads and steps on, never a harness fault and never a hang.
    pub exit_code: i32,
}

/// The binding's tool injection: extra definitions, and a router that
/// answers the invocations it owns.
///
/// **Router obligations**, which litany cannot enforce and therefore
/// states — [`route`](Self::route) runs *in the executor's own thread*,
/// so nothing in the harness can interrupt it:
///
/// - **It carries its own deadline.** litany imposes no wall-clock limit
///   on a tool (§3.3), and a subprocess's SIGTERM cascade has no
///   in-process analogue. Bound every wait, and render an expired one as
///   a non-zero [`RoutedCapture`].
/// - **A vanished endpoint is a result, not a hang and not a panic.**
///   Unreachable, disconnected, protocol garbage: all of them are
///   `exit_code != 0` with the reason on `stderr`, which is exactly what
///   an external tool that cannot reach its backend does.
/// - **It watches [`RoutedCall::stop`]** so a `litany stop` landing in a
///   routed invocation ends it as promptly as SIGTERM ends a subprocess.
///
/// The per-tool-call disk record (`input.json` / `output.json`, §3.3) is
/// **not** the router's to write: the executor lands it around every
/// answer, routed or spawned, so one convention holds for both and a
/// host cannot forget it (PRINCIPLES "Structure over discipline").
pub trait ToolInjection {
    /// The definitions this host splices into every request assembled
    /// while it is installed — read by the composer (the `tools: [...]`
    /// array) and by the grant gate (the effective toolset), so the two
    /// cannot disagree about what exists.
    ///
    /// Returning an empty list is the ordinary "nothing right now" and
    /// declares nothing; the injection is still installed.
    fn tools(&self) -> Vec<InjectedTool>;

    /// Answer `call`, or decline it. `Some` is this host's answer and
    /// ends the invocation; `None` falls through to the §3.3 resolution
    /// order (harness root, `PATH`, the in-process front door) exactly as
    /// if no injection were installed.
    ///
    /// Nothing checks that the names answered here are the names
    /// [`tools`](Self::tools) declares. A host *may* route a pool tool's
    /// name — it took that name over in the declaration too (the composer
    /// gives an injected definition precedence, so the model reads the
    /// schema of the thing that will actually run) — and it may decline a
    /// name it declared, which lands as an ordinary "no such tool"
    /// decline behind the front door.
    fn route(&self, call: RoutedCall<'_>) -> Option<RoutedCapture>;
}
