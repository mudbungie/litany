//! Every way driving an agent can fail (ARCH §2, §4.4, §6).
//!
//! One taxonomy for the whole executor — the step loop, the config
//! reads, the adapter, the dispatch gate, the inbox — deliberately
//! narrower than brazen's: wire-level distinctions are brazen's, spoken
//! in band as the `CanonicalError` this enum folds into
//! [`Error::AdapterError`] (§4.4). It lives beside [`super::run`] rather
//! than inside it because it is the module's shared vocabulary, not one
//! function's.

mod from_adapter;

use super::{budget, dispatch, fork_point, inbox};
use crate::prompt::ExecError;
use std::path::PathBuf;
use thiserror::Error as ThisError;

/// Every way [`run`] can fail. The taxonomy is intentionally narrower
/// than brazen's: wire-level distinctions are brazen's, surfaced in-band
/// as the `CanonicalError` this enum folds into [`Error::AdapterError`].
#[derive(Debug, ThisError)]
// `AdapterError` deliberately keeps the suffix; renaming would churn every
// call site for no clarity. The lint only surfaced once the §3.4 narrowing
// made this enum non-exported.
#[allow(clippy::enum_variant_names)]
pub enum Error {
    #[error("config: {0}")]
    Config(#[from] crate::config::LoadError),
    #[error("harness root: {0}")]
    HarnessRoot(#[from] crate::harness_root::Error),
    #[error("providers.yaml has no {0:?} role")]
    RoleMissing(String),
    #[error(transparent)]
    Layout(#[from] crate::workspace::LayoutError),
    /// The start named a fork point that resolves to nothing, or named
    /// two (§2.3, [`fork_point`]) — declined before the branch exists.
    #[error(transparent)]
    ForkPoint(#[from] fork_point::Error),
    #[error("read control {path} from the config commit (ARCH §2.2): {source}")]
    ControlRead {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(
        "model id {0:?} collides with the reserved transcript origin token `tool` (ARCH §2.3); \
         rename the model row"
    )]
    ReservedModelId(String),
    #[error("i/o writing conversation artifact: {0}")]
    Io(#[from] std::io::Error),
    #[error("adapter subprocess: {0}")]
    AdapterSpawn(#[source] std::io::Error),
    /// The adapter binary is not there. `NotFound` at the spawn is the
    /// one launch failure the user can act on, and the first real
    /// command of every binary-install user hits it — neither `cargo
    /// install litany` nor the release tarball lays down `bz`. So it
    /// gets the version guard's voice rather than a bare errno: the
    /// binary, the fact, the section, and the literal fix-it command
    /// carrying the linked pin ([`brazen_pin`], the number's one home).
    /// The errno trails as detail.
    #[error(
        "provider adapter {binary:?} not found (ARCH §4.4 — the default adapter is `bz` on \
         PATH; install the pinned binary: cargo install brazen --version ={pin} --locked, \
         or name an adapter you have with `adapter:` in the harness root's models.yaml): \
         {source}"
    )]
    AdapterMissing {
        binary: String,
        pin: String,
        #[source]
        source: std::io::Error,
    },
    #[error("tool {tool}: {source}")]
    ToolExec {
        tool: String,
        #[source]
        source: ExecError,
    },
    /// The configured tool control could not adjudicate — it would not
    /// spawn, crashed, or broke the verdict protocol (§3.3 *Tool
    /// control*). **Fails closed**: the invocation never executes and
    /// the step aborts loudly. Closed because a control exists to keep
    /// an invocation from running unreviewed — failing open would run
    /// exactly what the operator asked to gate; loud rather than
    /// in-band because a broken control is operator infrastructure the
    /// model can neither see nor fix, and an in-band decline would
    /// invite blind retries against it.
    #[error(
        "tool control {command:?} failed adjudicating {tool:?}: {detail} — a control that \
         cannot answer fails closed: the invocation does not run and the step aborts \
         (ARCH §3.3 Tool control); fix the control or remove the workflow's tool_control: block"
    )]
    ToolControl {
        command: String,
        tool: String,
        detail: String,
    },
    #[error("adapter emitted malformed v=1 event JSON: {0}")]
    AdapterJson(#[source] serde_json::Error),
    /// A provider failure brazen spoke in band (§4.4), carrying the
    /// **provider row** the model call was routed to. The row is litany's own
    /// fact — a role's `provider:` in the config commit's
    /// `providers.yaml` (§4.3) — and brazen, which owns endpoints and
    /// auth, never learns which name litany knows it by, so naming it is
    /// this side's job: a workspace binds several rows, and a decline
    /// that names none of them says nothing about where to look.
    #[error("provider error ({kind}) on provider row {row:?}: {message}")]
    AdapterError {
        kind: String,
        row: String,
        message: String,
    },
    /// The credential-shaped case of the above — brazen's `auth` kind,
    /// which it normalizes a 401/403 into. It is split out for the same
    /// reason [`Error::AdapterMissing`] is split from `AdapterSpawn`: it
    /// is the one provider failure a first-run user is *certain* to hit
    /// and can act on unaided, so it gets a remedy rather than a
    /// classification. brazen's own message already names the remedies —
    /// but as `bz --login --provider <id>`, a literal placeholder,
    /// because the row id is exactly what it cannot supply. This variant
    /// substitutes it.
    #[error(
        "provider error (auth) on provider row {row:?}: {message} — no credential is \
         reaching that row; authenticate it with `bz --login --provider {row}`, or export \
         the API-key env var it is configured to read. `bz --list-providers` shows every \
         row's auth mode and credential state, and the row a role uses is its `provider:` \
         in the config commit's providers.yaml (ARCH §4.3). Auth and endpoints are \
         brazen's alone — the harness never sees credential material (ARCH §4.4)"
    )]
    AdapterAuth { row: String, message: String },
    #[error(
        "adapter stream ended without a terminal `end` (killed mid-stream, ARCH §2.9); \
         adapter stderr tail: {tail} (full capture: {stderr_log})"
    )]
    AdapterHalfStream { stderr_log: PathBuf, tail: String },
    #[error(
        "bz version {found:?} does not match the linked brazen crate {expected:?} \
         (ARCH §4.4 — install the pinned binary: cargo install brazen --version ={expected})"
    )]
    VersionSkew { found: String, expected: String },
    #[error("adapter-override handshake failed: MessageStart.v={found:?}, expected {expected}")]
    HandshakeMismatch { found: Option<u8>, expected: u8 },
    #[error("git {op}: {source}")]
    Git {
        op: &'static str,
        #[source]
        source: std::io::Error,
    },
    /// The §6 budget gate refused a dispatch before it forked
    /// ([`child_dispatch::run`]) — the declared ceiling would be breached
    /// by the child that does not exist yet. Distinct from the
    /// `budget-exhausted` *terminal* state (§6, [`budget::mark_exhausted`]),
    /// which retires a branch that already exists: nothing was created
    /// here, so there is no branch to mark and no epitaph to deposit.
    #[error(
        "dispatch of {child} from {parent} refused: {exhausted} (ARCH §6 budgets — \
         the limit is declared in the governing config's workflow.yaml)"
    )]
    DispatchRefused {
        child: String,
        parent: String,
        exhausted: budget::Exhausted,
    },
    /// A grant the governing config commit does not describe (§3.3),
    /// refused at the fork rather than composed into a smaller toolset.
    #[error(transparent)]
    GrantUndescribed(#[from] dispatch::Undescribed),
    /// A `--name` malformed, id-shaped or taken (§2.3) — refused pre-fork.
    #[error(transparent)]
    NameUnavailable(#[from] crate::workspace::agent_name::Unavailable),
    /// The hop's target has no `agents/*` ref — the shared existence
    /// decline ([`crate::workspace::require_agent`]), fired *before* the
    /// lease so the refusal leaves no inbox directory behind. It is
    /// deliberately distinct from the §2.11 lost-lease no-op: that one
    /// is a live agent already being driven, this one is no agent at all.
    #[error(transparent)]
    UnknownAgent(#[from] crate::workspace::UnknownAgent),
    #[error("acquire executor lock on inbox {path}: {source}")]
    ExecutorLock {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(
        "adopt LITANY_LOCK_FD lease for {agent}: {detail} — a bad fd means a defective \
         launcher; declined, never silently reacquired (ARCH §6)"
    )]
    LeaseAdopt { agent: String, detail: String },
    #[error(
        "branch {branch} tip is an assistant entry with tool_use unmatched by committed \
         tool results — a mid-step crash after the assistant entry landed; tool side \
         effects are not replayable, so this is declined (ARCH §6). Recover by \
         fork-from-history (ARCH §2.3)."
    )]
    UnpairedToolUse { branch: String },
    #[error(
        "workflow action {action:?} is not yet interpreted at the {event} event (ARCH §6 \
         binding interpreter — shipped subset is the terminal ref marks); the action is \
         in the closed set but its executor is a tracked follow-on of bl-6a3b"
    )]
    ActionUnsupported { action: String, event: &'static str },
    #[error("deposit initial user message: {0}")]
    Deposit(#[from] inbox::DepositError),
    /// A selected `summary/**` entry carries git conflict-marker lines
    /// (§5.2 marker guard). The write path promises this can never
    /// happen — the compaction landing declines any content conflict
    /// during the replay (§2.6) — so a marked summary is a violated
    /// invariant, and composing it would send corrupted context into
    /// every subsequent model call on the branch (§2.7). Refused
    /// loudly, naming the path; recovery is §5.4 deletion or repair.
    #[error(
        "assembly refused {path}: it carries git conflict-marker lines, which the ARCH §2.6 \
         compaction-landing decline promises summary/** never does; composing it would \
         corrupt the branch's context (ARCH §2.7). Repair or delete the file (ARCH §5.4) \
         to resume"
    )]
    SummaryConflictMarkers { path: String },
    /// A compactor nominated a path that is **not compaction-eligible**
    /// (§2.7, §2.8): what is not the branch's history is not a pass's to
    /// shed. Three classes qualify, and `what` names the one that fired.
    /// The **dispatch entry** is the transcript entry the opening prompt
    /// landed as (§2.3, §2.11) — the same text rides `goal.md` and is
    /// quoted verbatim into the compactor's own goal, so it reads as pure
    /// duplication to a model told to nominate superseded files, while
    /// deleting it deletes the operator's only copy of the prompt the
    /// conversation exists to serve. The **system slot's files**
    /// (`goal.md`, `soul.md`, `name` — §5.2 structural wire homes) are
    /// worse when it fires: the landing would `git rm` them from the
    /// *dispatching* branch, which then keeps stepping with no goal, no
    /// soul or no identity line on every later model call. **This pass's
    /// own product** is worst of all (bl-c7bb): the summary is the only
    /// thing the compacted span leaves behind, so a landing carrying a
    /// `git rm` of it carries away the whole history it was dispatched to
    /// preserve.
    #[error(
        "{path} is {what}, and is not compaction-eligible (ARCH §2.7). Nominate a later \
         transcript entry, an earlier pass's superseded summary/, or a spent skills/ body \
         instead"
    )]
    NotCompactionEligible { path: String, what: String },
    /// The workflow names the `window_percent` checkpoint trigger, but
    /// the branch's last usage carries no context window for the model
    /// that authored it (`docs/DESIGN_CONTEXT_ECONOMY.md` §5.1). The
    /// window is brazen's fact, delivered in band on the `Usage` event
    /// and recorded beside the counters (§2.3, §4.2 — litany keeps no
    /// per-model table); a row brazen cannot state one for leaves the
    /// field absent. Declined at the boundary rather than answered "not
    /// due", because the alternative is a configured trigger that never
    /// fires and says nothing about why.
    #[error(
        "compaction trigger `window_percent` has no context window to measure against: the \
         last usage on this branch, from model {model:?}, carries none (ARCH §2.3 — the \
         window rides brazen's Usage event; litany keeps no per-model table, §4.2). Name a \
         model whose provider reports a context window, or choose another \
         compaction.intermediate.trigger"
    )]
    CompactionWindowUnknown { model: String },
    #[error("tool {name} schema unreadable at {path}: {source}")]
    ToolSchemaIo {
        name: String,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("tool {name} schema at {path} is not valid JSON: {source}")]
    ToolSchemaJson {
        name: String,
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("tool {name} skill frontmatter unreadable at {path}: {source}")]
    SkillFrontmatterIo {
        name: String,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("tool {name} skill frontmatter at {path} is malformed: {source}")]
    SkillFrontmatter {
        name: String,
        path: PathBuf,
        #[source]
        source: serde_yaml_ng::Error,
    },
}
