//! The **closed set of built-in tool names** and its human rendering
//! (ARCH §3.3, §12 v0.3 toolset) — the one list behind the unknown-tool
//! decline, `litany tool --help`, and the `cmd::BUILTIN_TOOLS` seam an
//! injecting host asks rather than restates.
//!
//! Split out of [`super`] at the per-file cap (bl-3c11). The seam is
//! real: this is *what the engine answers to*, a fact consumers read,
//! while the module it left is *how each name is performed*. Every name
//! stays reachable at `builtin::*`, which is the path both the router
//! and the surface already spell.

/// Built-in tool name: atomic multi-file structured edit (§3.3 *The
/// patch tool*).
pub(super) const APPLY_PATCH: &str = "apply_patch";
/// Built-in tool name: run a shell command (§3.3).
pub(super) const BASH: &str = "bash";
/// Built-in tool name: move the calling agent's working directory (§3.3
/// *Working directory*).
pub(super) const CD: &str = "cd";
/// Built-in tool name: spawn a subagent (§2.5).
pub(super) const DISPATCH: &str = "dispatch";
/// Built-in tool name: copy a pooled skill body into the worktree (§3.3
/// Body-on-demand).
pub(super) const LOAD_SKILL: &str = "load_skill";
/// Built-in tool name: deposit into an existing agent's inbox (§2.11).
pub(super) const MESSAGE: &str = "message";
/// Built-in tool name: run a model-authored python3 program that
/// composes this agent's own tools (`docs/DESIGN_CODE_EXECUTION.md`
/// §2.2). `pub(crate)` and not private: it is also the name the door
/// refuses at depth 1 ([`crate::prompt::dispatch::door::COMPOSING`]),
/// and one name has one home.
pub(crate) const PYTHON: &str = "python";
/// Built-in tool name: read a file's bytes (§3.3).
pub(super) const READ_FILE: &str = "read_file";
/// Built-in tool name: propose one durable fact onto the lineage's
/// `facts.md` (`docs/DESIGN_CONTEXT_ECONOMY.md` §3).
pub(super) const REMEMBER: &str = "remember";
/// Built-in tool name: search the workspace's stored transcript entries
/// (§3.3, `docs/DESIGN_CONTEXT_ECONOMY.md` §4).
pub(super) const SEARCH_HISTORY: &str = "search_history";

/// The closed set of built-in tool names `litany tool <name>` answers to,
/// sorted — the one list behind both the [`Error::Unknown`] decline and
/// the `<NAME>` argument's CLI help (PRINCIPLES single source of truth).
/// The compactor pair (`write_summary` / `mark_for_deletion`) is
/// deliberately absent: it is injected for the compactor role alone
/// (§2.7), never a name a general agent or an operator elects, so it is
/// routed but not advertised.
pub const NAMES: [&str; 10] = [
    APPLY_PATCH,
    BASH,
    CD,
    DISPATCH,
    LOAD_SKILL,
    MESSAGE,
    PYTHON,
    READ_FILE,
    REMEMBER,
    SEARCH_HISTORY,
];

/// [`NAMES`] rendered for a human: the pool named in the unknown-tool
/// decline and in `litany tool --help`, in the same voice `load_skill`
/// names its own pool with (§3.3 "declined … naming the available pool").
pub fn pool() -> String {
    NAMES.join(", ")
}
