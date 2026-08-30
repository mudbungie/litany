//! The command-surface parity checker (ARCH §3.4, CI-enforced both
//! directions).
//!
//! "The crate exposes nothing public that is not a verb's entry, its
//! arguments, its products, or the binding preludes … and no verb lacks
//! its entry." This test is the enforcement mechanism for that invariant.
//! It rides ordinary `cargo test` → tarpaulin → `make check` → the
//! pre-commit hook and GitHub Actions, with no new toolchain.
//!
//! It is itself a consumer of the public surface — it may link nothing
//! but [`litany::cmd`]. The ground truth of "what is `pub`" is the
//! crate's own source, parsed with `syn`; the ground truth of "what is a
//! verb" is the CLI's introspected subcommand set, read from clap at
//! runtime. A bijection between the two is the invariant.
//!
//! The bijection is enforced at three depths, one module each:
//!
//! - [`entries`] — **entry parity, by construction.** A verb table pairs
//!   each `Command` variant's constructor with its module's `run` as
//!   function *values*, so the compiler — not an assertion — proves the
//!   variant's payload type, the entry's argument type, the injected
//!   [`litany::cmd::Fx`] and the product type `Result<Outcome, Error>`
//!   are one and the same. Divergence is unrepresentable rather than
//!   checked. The table is itself asserted exhaustive against both the
//!   clap subcommand set and the crate's verb modules, so it cannot rot.
//! - [`surface`] — **declaration parity, totally.** Every externally
//!   reachable declaration in the crate (walked through the module
//!   graph, [`graph`], and extracted member-deep by [`items`]) must equal
//!   an expected set that is *computed* from clap wherever a verb's
//!   surface is derived from the CLI, and enumerated only for the fixed
//!   binding seam. Publicity is a property of a path, so the walk starts
//!   at the crate roots and must meet every `src/**/*.rs`: nothing can
//!   hide in a file the checker never opened, in a nested module, in a
//!   type's fields, or in an `impl` block.
//! - [`arguments`] — **argument parity, shape-deep.** For every verb, the
//!   clap-introspected argument set must correspond 1:1 to the public
//!   fields of that verb's `Args` — by name, by arity (flag / optional /
//!   required) and by form (named vs positional). The struct is shared
//!   between the bindings, so this is what "parameter identity holds by
//!   construction" asserts about its own construction.
//!
//! One further leg is the compiler's own, and is why a name-level check
//! of fields and signatures suffices: rustc's `private_interfaces` lint —
//! warn-by-default, promoted to an error by `make check`'s `clippy -D
//! warnings` — rejects a private type appearing in a public field or
//! signature. No declaration on the surface can smuggle the crate's
//! internals out by type.

mod arguments;
mod entries;
mod graph;
mod items;
mod surface;
