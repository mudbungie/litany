//! Litany — a git-backed agent harness, exposed as its command surface.
//!
//! The crate's public API is [`cmd`] (ARCH §3.4 "One command
//! surface, two bindings"): the `Cli`/`Command` clap definitions, one
//! **entry** per verb, the binding seam ([`cmd::Fx`], [`cmd::Outcome`],
//! [`cmd::Error`]), and the binding preludes ([`cmd::prelude`]) — plus
//! exactly one enumerated exception, the [`mint`] seam (the agent-name
//! mint yog draws through the crate, §2.3 / yog bl-aca4). Nothing
//! else is public. That is the parity invariant (§3.4, CI-enforced by
//! `tests/command_surface_parity/`): the library surface *is* the
//! command surface plus the mint seam — the crate exposes nothing
//! public that is not a verb's entry, its arguments, its products, the
//! binding preludes, or the mint seam, and no verb lacks its entry.
//!
//! Consume it two ways, both the *same* control plane: exec the `litany`
//! binary (the exec binding, `src/bin/litany`) or link the crate and call
//! the same entries in-process (the linked binding, §3.5). The linked
//! binding promises **pin-exact 0.x consumption only** — no semver
//! stability, the posture brazen takes toward litany (§4.4).
//!
//! Everything below `cmd` is private machinery, reachable only through a
//! verb's entry — pub-in-private, so externally unreachable:
//! - `config`: parses and validates the config-commit files (§2.2) and
//!   generates their JSON Schemas (`config::schemas`).
//! - `harness_root`: resolves the XDG-split installation root, collapsed
//!   by `LITANY_HOME` (§2.2).
//! - `install`: founds that root, seed-if-absent — the `prime` verb (§2.2).
//! - `workspace`: the workspace physical model — the bare `repo.git`, the
//!   `config/*` / `agents/*` ref namespaces, governing-config resolution
//!   (§2.2–§2.3).
//! - `template`: the config-commit skeleton `litany new` authors from.
//! - `prompt`: the executor — steps, tools, dispatch, inbox, stop (§2, §6).
//! - `provider`: the response-segment classifier over brazen's `v=1`
//!   event vocabulary (§4.4).
//! - `archive`: bundle/replay of an agent subtree (§9.2).
//! - `skill`: the skill-pool descriptor surface (§3.3).

mod archive;
pub mod cmd;
mod config;
mod harness_root;
mod install;
pub mod mint;
mod name;
mod prompt;
mod provider;
mod skill;
mod template;
mod workspace;

#[cfg(test)]
mod e2e;
#[cfg(test)]
mod test_support;
