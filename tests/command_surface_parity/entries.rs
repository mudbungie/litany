//! Entry parity, by construction: each verb's [`Command`] variant and its
//! module's `run` are *paired as values*, so the compiler proves what an
//! assertion would otherwise have to check.
//!
//! [`pair`] takes the variant's tuple constructor (`fn(A) -> Command`) and
//! the module's entry (`fn(A, &mut Fx) -> Result<Outcome, Error>`) with
//! one shared type parameter `A`. Passing `Command::New` and `new::run`
//! therefore compiles only if:
//!
//! - the variant's payload type *is* the entry's argument type — so the
//!   argument struct the CLI parses into is the very struct the linked
//!   binding constructs (ARCH §3.4 "parameter identity … by construction",
//!   here made a proof rather than a claim); and
//! - the entry takes the binding's [`Fx`] injections and yields the shared
//!   product type `Result<Outcome, Error>` — so every verb's product is
//!   the same public [`Outcome`], performed identically by either binding.
//!
//! Divergence is unrepresentable, not merely detected. What a table *can*
//! do is go stale, so [`the_verb_table_is_exhaustive`] asserts it against
//! both directions of the bijection at once: the clap subcommand set, and
//! the crate's public verb modules with their `Command` variants.

use crate::graph;
use crate::items::is_pub;
use clap::CommandFactory;
use litany::cmd::{Cli, Command, Error, Fx, Outcome};
use std::collections::BTreeSet;
use syn::Item;

/// The pairing itself: one type parameter, two function values.
fn pair<A>(
    _variant: fn(A) -> Command,
    _entry: fn(A, &mut Fx) -> Result<Outcome, Error>,
) -> &'static str {
    "paired"
}

/// The verb table: `Variant => module`, once. Everything below reads it.
macro_rules! verb_table {
    ($($variant:ident => $module:ident),+ $(,)?) => {
        /// Every verb as `(variant, module)`.
        pub const VERBS: &[(&str, &str)] = &[$((stringify!($variant), stringify!($module))),+];

        /// Pair every verb's variant with its entry — the compile-time
        /// half of the bijection. Called by a test so the pairing is
        /// executed, not merely compiled.
        fn pair_every_verb() -> usize {
            [$(pair(Command::$variant, litany::cmd::$module::run)),+].len()
        }
    };
}

verb_table! {
    New => new,
    Config => config,
    Prompt => prompt,
    Dispatch => dispatch,
    Retarget => retarget,
    Workflow => workflow,
    Stop => stop,
    Message => message,
    Proposal => proposal,
    Scan => scan,
    Skills => skills,
    Bundle => bundle,
    Delete => delete,
    Replay => replay,
    Advance => advance,
    Invoke => invoke,
    Tool => tool,
    Prime => prime,
}

/// The subcommand names clap reports for the shared [`Cli`] at runtime —
/// the ground truth of "what is a verb".
pub fn cli_subcommands() -> BTreeSet<String> {
    Cli::command()
        .get_subcommands()
        .map(|c| c.get_name().to_string())
        .collect()
}

/// `cmd`'s public modules that are **not** verbs, and are stated as such
/// in the seam ledger ([`crate::surface`]): the mechanisms a binding
/// performs before a driver verb, and the three types it speaks to a
/// verb in. Every other public module under `cmd` is a verb, and the
/// exhaustiveness assertion below is what makes that true in both
/// directions.
const NON_VERB_MODULES: &[&str] = &["prelude", "seam"];

/// The public modules of `cmd` that are verb modules — its whole public
/// module set minus [`NON_VERB_MODULES`].
pub fn verb_modules() -> BTreeSet<String> {
    graph::crate_graph()
        .iter()
        .filter(|m| m.public && !m.test_only)
        .filter_map(|m| m.path.strip_prefix("crate::cmd::").map(String::from))
        .filter(|name| !NON_VERB_MODULES.contains(&name.as_str()))
        .collect()
}

/// `Command`'s variants, parsed from the source — the third view of the
/// verb set, independent of both clap and the module tree.
fn command_variants() -> BTreeSet<String> {
    let graph = graph::crate_graph();
    let cmd = graph::module(&graph, "crate::cmd");
    let variants = cmd.items.iter().find_map(|item| match item {
        Item::Enum(e) if e.ident == "Command" && is_pub(&e.vis) => Some(&e.variants),
        _ => None,
    });
    variants
        .expect("`pub enum Command` in src/cmd/mod.rs")
        .iter()
        .map(|v| {
            assert!(
                matches!(&v.fields, syn::Fields::Unnamed(f) if f.unnamed.len() == 1),
                "Command::{} must be a single-field tuple variant carrying its verb's Args",
                v.ident,
            );
            v.ident.to_string()
        })
        .collect()
}

#[test]
fn every_verb_entry_is_its_variants_payload() {
    assert_eq!(pair_every_verb(), VERBS.len());
}

/// The `prelude` re-exports widen the surface by three *mechanisms* and
/// nothing else. A re-export publishes whatever it names — a type would
/// bring its whole structure onto the surface through a chain the
/// declaration walk sees only the leaf name of — so each is pinned here
/// to its exact type: two plain `fn()`s a binding can only call (the same
/// `fn()` [`Command::preludes`] hands out), and the flag accessor whose
/// value [`Fx::stop`] carries. They are named, never invoked: performing
/// them is the binding's act (ARCH §3.4).
#[test]
fn the_prelude_re_exports_are_mechanisms_and_no_types() {
    use litany::cmd::prelude;
    use std::sync::atomic::{AtomicBool, Ordering};
    let mechanisms: [fn(); 2] = [prelude::become_pgid_leader, prelude::install_stop_handler];
    let flag: fn() -> &'static AtomicBool = prelude::stop_flag;
    assert_eq!(mechanisms.len(), 2);
    assert!(!flag().load(Ordering::SeqCst), "no stop signalled here");
}

/// The table names every verb and nothing but: its modules are exactly
/// the CLI's subcommands and exactly `cmd`'s public verb modules, and its
/// variants are exactly `Command`'s. So a verb added to any one of the
/// three without the others — or without its compile-checked pairing —
/// fails here.
#[test]
fn the_verb_table_is_exhaustive() {
    let modules: BTreeSet<String> = VERBS.iter().map(|(_, m)| (*m).to_string()).collect();
    assert_eq!(modules, cli_subcommands(), "verb table vs CLI subcommands");
    assert_eq!(modules, verb_modules(), "verb table vs cmd's verb modules");
    let variants: BTreeSet<String> = VERBS.iter().map(|(v, _)| (*v).to_string()).collect();
    assert_eq!(
        variants,
        command_variants(),
        "verb table vs Command variants"
    );
    for (variant, module) in VERBS {
        assert_eq!(
            &variant.to_lowercase(),
            module,
            "Command::{variant} and module cmd::{module} are mispaired",
        );
    }
}
