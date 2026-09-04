//! Declaration parity: the crate's *entire* externally reachable
//! surface, member-deep, equals the command surface — one set equality.
//!
//! The expected side is computed wherever the surface is derived from the
//! CLI (a verb module's `Args`, its fields, its `run`, its `Command`
//! variant) and enumerated only for the fixed seams — the binding seam
//! ([`Cli`], [`Command`], [`Outcome`], [`Fx`], [`Error`] and the
//! `prelude` re-exports) and the mint seam (`crate::mint`, ARCH §2.3 /
//! yog bl-aca4). So adding a verb needs no edit here, while widening a
//! seam by one field, one method, one variant or one derive fails until
//! the widening is stated as such.
//!
//! Three assertions close the ways a surface could grow *unseen* rather
//! than merely unstated: every source file must be met by the module walk
//! ([`every_source_file_is_reachable`]), `cmd` may hold no private nook
//! for an `impl` to hide in, and no module below the surface may name it
//! ([`nothing_below_the_surface_reaches_into_it`]).

use crate::arguments::arg_ids;
use crate::entries::VERBS;
use crate::graph::{self, Module};
use crate::items::{entries, exported_macros};
use std::collections::BTreeSet;
use syn::{Item, UseTree};

/// The binding seam, enumerated: the parts of `cmd` that are not derived
/// from the verb set. Every line is a deliberate public commitment.
///
/// **Two of `cmd`'s modules are not verbs**, and both are named here as
/// `mod` entries so a third cannot appear unstated: `prelude` (the
/// mechanisms a binding performs, ARCH §3.4) and `seam` (the three types
/// it speaks to a verb in — split out of `cmd/mod.rs` when the verb list
/// grew past the per-file cap, bl-9a65). The types are declared in
/// `seam` and re-exported at `cmd::*`, so both paths are stated: the
/// declaration where it lives, the `use` where every consumer names it.
const SEAM: &[&str] = &[
    "struct Cli",
    "derive Cli: clap::Parser",
    "derive Cli: Debug",
    "field Cli.command",
    "enum Command",
    "derive Command: clap::Subcommand",
    "derive Command: Debug",
    "method Command::preludes",
    "method Command::run",
    // The host tool-injection seam (ARCH §3.3 *Host-injected tools*):
    // `Fx.tool_injection`'s trait and the two plain data types its two
    // methods speak in. Re-exported from `prompt::tool::inject` rather
    // than declared here, because the halves it drives — prompt
    // assembly and the executor — are below the surface and may not
    // name it.
    "use InjectedTool",
    "use RoutedCall",
    "use RoutedCapture",
    "use ToolInjection",
    // The companion fact to that seam (ARCH §3.3, bl-4cbb): an injecting
    // host answers every name itself, so the set the engine can perform
    // must be readable rather than restated downstream. Re-exported for
    // the same reason — `prompt::tool::builtin` is below the surface.
    "use BUILTIN_TOOLS",
    "mod prelude",
    "mod seam",
    "use Outcome",
    "use Fx",
    "use Error",
];

/// The binding seam's own declarations, in the module that holds them
/// (`cmd::seam`): the product, the injections and the failure.
const SEAM_TYPES: &[&str] = &[
    "enum Outcome",
    "derive Outcome: Debug",
    "variant Outcome::Line",
    "variant Outcome::Quiet",
    "variant Outcome::Exec",
    "variant Outcome::Code",
    "struct Fx",
    "field Fx.driver_target",
    "field Fx.adapter_target",
    "field Fx.editor",
    "field Fx.tool_stdin",
    "field Fx.tool_stdout",
    "field Fx.tool_stderr",
    "field Fx.stop",
    "field Fx.tool_injection",
    "struct Error",
    "derive Error: Debug",
    "method Error::new",
    "impl Error: std::fmt::Display",
    "impl Error: std::error::Error",
];

/// The three mechanisms `cmd::prelude` re-exports (ARCH §3.4).
const PRELUDES: &[&str] = &["become_pgid_leader", "install_stop_handler", "stop_flag"];

/// The mint seam (ARCH §2.3 / §3.4, yog bl-aca4): the agent-name mint
/// the linked consumer draws through `crate::mint` — the pure function,
/// its injected RNG trait, the production generator, and the loud
/// exhaustion error. The wordlist is deliberately absent: the interface
/// is the function.
const MINT: &[&str] = &["use MintError", "use Rng", "use SplitMix64", "use mint"];

/// The whole expected public surface, module-qualified.
fn expected() -> BTreeSet<String> {
    let mut want = BTreeSet::from(["crate::mod cmd".to_string(), "crate::mod mint".to_string()]);
    want.extend(SEAM.iter().map(|e| format!("crate::cmd::{e}")));
    want.extend(SEAM_TYPES.iter().map(|e| format!("crate::cmd::seam::{e}")));
    want.extend(MINT.iter().map(|e| format!("crate::mint::{e}")));
    want.extend(
        PRELUDES
            .iter()
            .map(|p| format!("crate::cmd::prelude::use {p}")),
    );
    for (variant, verb) in VERBS {
        want.insert(format!("crate::cmd::mod {verb}"));
        want.insert(format!("crate::cmd::variant Command::{variant}"));
        for entry in [
            "struct Args",
            "derive Args: clap::Args",
            "derive Args: Debug",
            "fn run",
        ] {
            want.insert(format!("crate::cmd::{verb}::{entry}"));
        }
        // A verb's arguments are its `Args` fields — and the CLI's own
        // introspected argument set says which those must be.
        want.extend(
            arg_ids(verb)
                .iter()
                .map(|id| format!("crate::cmd::{verb}::field Args.{id}")),
        );
    }
    want
}

/// Every externally reachable declaration the crate actually has.
fn actual(graph: &[Module]) -> BTreeSet<String> {
    graph
        .iter()
        .filter(|m| m.public && !m.test_only)
        .flat_map(|m| {
            entries(&m.items)
                .into_iter()
                .map(|e| format!("{}::{e}", m.path))
        })
        .collect()
}

#[test]
fn the_public_surface_is_exactly_the_command_surface() {
    let graph = graph::crate_graph();
    let (got, want) = (actual(&graph), expected());
    let leaked: Vec<&String> = got.difference(&want).collect();
    let missing: Vec<&String> = want.difference(&got).collect();
    assert!(
        leaked.is_empty() && missing.is_empty(),
        "the public surface is not the command surface (ARCH §3.4)\n\
         leaked (public, but not a verb's entry, arguments, products or preludes): {leaked:?}\n\
         missing (the command surface expects it, the crate does not expose it): {missing:?}",
    );
}

/// The walk is total over `src/`: a file the checker never opened could
/// hold anything, so there must be no such file.
#[test]
fn every_source_file_is_reachable() {
    let mut visited: BTreeSet<_> = graph::crate_graph()
        .iter()
        .map(|m| m.file.clone())
        .collect();
    visited.extend(graph::binding_graph().iter().map(|m| m.file.clone()));
    let orphans: Vec<_> = graph::source_files()
        .difference(&visited)
        .cloned()
        .collect();
    assert!(
        orphans.is_empty(),
        "source files no `mod` declaration reaches, so the parity walk never opened them: {orphans:?}",
    );
}

/// Where an item names the command surface by an absolute path — the
/// only route to it from outside `cmd`, since a `super`-relative path can
/// only be written inside `cmd`, which holds no private module.
fn surface_references(items: &[Item]) -> Vec<String> {
    let mut out = Vec::new();
    for item in items {
        match item {
            Item::Use(u) => use_paths(&u.tree, String::new(), &mut out),
            Item::Impl(i) => out.extend(type_path(&i.self_ty)),
            Item::Type(t) => out.extend(type_path(&t.ty)),
            _ => {}
        }
    }
    out.retain(|p| p.starts_with("crate::cmd"));
    out
}

/// A type written as a plain path, rendered.
fn type_path(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Path(p) => Some(
            p.path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>()
                .join("::"),
        ),
        _ => None,
    }
}

/// Flatten a `use` tree into the full paths it imports.
fn use_paths(tree: &UseTree, prefix: String, out: &mut Vec<String>) {
    let join = |name: &syn::Ident| {
        if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}::{name}")
        }
    };
    match tree {
        UseTree::Path(p) => use_paths(&p.tree, join(&p.ident), out),
        UseTree::Name(n) => out.push(join(&n.ident)),
        UseTree::Rename(r) => out.push(join(&r.ident)),
        UseTree::Glob(_) => out.push(format!("{prefix}::*")),
        UseTree::Group(g) => g
            .items
            .iter()
            .for_each(|t| use_paths(t, prefix.clone(), out)),
    }
}

/// The surface cannot be widened from below: `cmd` has no private module
/// for an `impl` to hide in, nothing outside it names it (so no module
/// elsewhere can carry an inherent `impl` on a surface type), and no
/// module exports a macro (which would land at the crate root whatever
/// its module's own visibility).
#[test]
fn nothing_below_the_surface_reaches_into_it() {
    for m in graph::crate_graph().iter().filter(|m| !m.test_only) {
        if let Some(inner) = m.path.strip_prefix("crate::cmd") {
            assert!(
                m.public,
                "cmd has a private module `cmd{inner}` — the surface must be all public, so that every `impl` on it is walked"
            );
            continue;
        }
        let named = surface_references(&m.items);
        assert!(
            named.is_empty(),
            "{} names the command surface ({named:?}); nothing below the surface may reach into it",
            m.path,
        );
        let macros = exported_macros(&m.items);
        assert!(
            macros.is_empty(),
            "{} exports {macros:?}, which is public at the crate root",
            m.path,
        );
    }
}

/// The reference scan sees every way an item can name the surface, and
/// nothing that merely resembles one.
#[test]
fn the_reference_scan_finds_every_way_to_name_the_surface() {
    let src = "use crate::cmd::Outcome; use crate::cmd::{Fx, Error as E}; use crate::cmd::*; \
               use crate::prompt::step; use std::io; \
               impl crate::cmd::Outcome { pub fn leak() {} } \
               type Alias = crate::cmd::Error; type Fine = std::io::Error; \
               impl Local {} fn f() {}";
    assert_eq!(
        surface_references(&syn::parse_file(src).unwrap().items),
        [
            "crate::cmd::Outcome",
            "crate::cmd::Fx",
            "crate::cmd::Error",
            "crate::cmd::*",
            "crate::cmd::Outcome",
            "crate::cmd::Error",
        ],
    );
}
