//! Argument parity: for every verb, the CLI's introspected argument set
//! and the public fields of that verb's `Args` are the same set — by
//! name, by arity, and by form.
//!
//! The two bindings share the struct ([`crate::entries`] proves the
//! sharing at compile time), so this is the assertion *about the shared
//! struct itself*: that every field a linked binding can set is an
//! argument the CLI parses, and every argument the CLI parses is a field
//! a linked binding can set. That closes the divergences sharing alone
//! does not: a `#[arg(skip)]` field the CLI cannot supply, a private
//! field the linked binding cannot supply, an `Option` on one side and a
//! required value on the other, a flag that is positional in one reading
//! and named in the other.

use crate::entries::VERBS;
use crate::graph;
use crate::items::is_pub;
use clap::{Arg, ArgAction, CommandFactory};
use litany::cmd::Cli;
use std::collections::{BTreeMap, BTreeSet};
use syn::punctuated::Punctuated;
use syn::{Fields, FieldsNamed, Item, Meta, Token};

/// An argument's arity: a valueless flag, an optional value, or a value
/// that must be supplied.
#[derive(Debug, PartialEq, Eq)]
enum Shape {
    Flag,
    Optional,
    Required,
}

/// One argument as both sides must see it: its arity, and whether it is
/// named (`--flag`) rather than positional.
type Argument = (Shape, bool);

/// clap's own `--help`/`--version` belong to the parser, not to the verb.
fn is_verb_argument(arg: &Arg) -> bool {
    !matches!(
        arg.get_action(),
        ArgAction::Help | ArgAction::HelpShort | ArgAction::HelpLong | ArgAction::Version
    )
}

/// The clap subcommand a verb parses through.
fn verb_command(verb: &str) -> clap::Command {
    Cli::command()
        .find_subcommand(verb)
        .unwrap_or_else(|| panic!("`{verb}` is not a CLI subcommand"))
        .clone()
}

/// The argument names the CLI introspects for a verb.
pub fn arg_ids(verb: &str) -> BTreeSet<String> {
    verb_arguments(verb).into_keys().collect()
}

/// A verb's arguments, as clap reports them.
fn verb_arguments(verb: &str) -> BTreeMap<String, Argument> {
    verb_command(verb)
        .get_arguments()
        .filter(|a| is_verb_argument(a))
        .map(|a| {
            let shape = if !a.get_action().takes_values() {
                Shape::Flag
            } else if a.is_required_set() {
                Shape::Required
            } else {
                Shape::Optional
            };
            let named = a.get_long().is_some() || a.get_short().is_some();
            (a.get_id().to_string(), (shape, named))
        })
        .collect()
}

/// The arity a field's type declares: `bool` is a flag, `Option<_>` an
/// optional value, `Vec<_>` a repeatable optional one (clap's
/// zero-or-more reading — `prompt`/`dispatch` `--pin`), anything else a
/// required one. A type outside that mapping simply disagrees with
/// clap's reading of it, which is exactly the divergence this test
/// exists to report.
fn field_shape(ty: &syn::Type) -> Shape {
    let head = match ty {
        syn::Type::Path(p) => p
            .path
            .segments
            .last()
            .expect("argument type has a segment")
            .ident
            .to_string(),
        _ => panic!("an argument field must be a named type"),
    };
    match head.as_str() {
        "bool" => Shape::Flag,
        "Option" | "Vec" => Shape::Optional,
        _ => Shape::Required,
    }
}

/// Does the field's clap attribute make it named rather than positional?
fn is_named(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .filter(|a| a.path().is_ident("arg") || a.path().is_ident("clap"))
        .any(|a| {
            a.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
                .expect("clap argument attribute parses")
                .iter()
                .any(|m| m.path().is_ident("long") || m.path().is_ident("short"))
        })
}

/// A verb's arguments, as its `Args` struct declares them. Every field
/// must be `pub`: a field the linked binding cannot set is an argument
/// only one binding has.
fn field_arguments(fields: &FieldsNamed) -> BTreeMap<String, Argument> {
    fields
        .named
        .iter()
        .map(|f| {
            let name = f.ident.as_ref().expect("named field").to_string();
            assert!(
                is_pub(&f.vis),
                "`Args.{name}` is not `pub`, so the linked binding cannot supply it",
            );
            (name, (field_shape(&f.ty), is_named(&f.attrs)))
        })
        .collect()
}

/// The `pub struct Args` of one verb module.
fn args_fields(verb: &str) -> BTreeMap<String, Argument> {
    let graph = graph::crate_graph();
    let module = graph::module(&graph, &format!("crate::cmd::{verb}"));
    let args = module
        .items
        .iter()
        .find_map(|item| match item {
            Item::Struct(s) if s.ident == "Args" && is_pub(&s.vis) => Some(s),
            _ => None,
        })
        .unwrap_or_else(|| panic!("cmd::{verb} must expose `pub struct Args`"));
    let Fields::Named(named) = &args.fields else {
        panic!("cmd::{verb}::Args must have named fields — the linked binding sets them by name")
    };
    field_arguments(named)
}

#[test]
fn every_verb_argument_is_a_field_of_its_entrys_args() {
    for (_, verb) in VERBS {
        assert_eq!(
            args_fields(verb),
            verb_arguments(verb),
            "verb `{verb}`: its `Args` fields and its CLI arguments diverge \
             (name, arity, named-vs-positional)",
        );
    }
}

/// The field reading covers every argument shape the derive can carry,
/// including the attribute forms the crate happens not to use today.
#[test]
fn the_field_reading_covers_every_argument_shape() {
    let src = "struct Args { pub positional: PathBuf, pub optional: Option<String>, \
               #[arg(long)] pub flag: bool, #[clap(short, long = \"renamed\")] pub named: u8, \
               #[arg(value_parser = parser)] pub plain: String, \
               #[arg(long)] pub many: Vec<String> }";
    let mut items = syn::parse_file(src).unwrap().items;
    let Item::Struct(s) = items.remove(0) else {
        panic!("a struct")
    };
    let Fields::Named(named) = &s.fields else {
        panic!("named fields")
    };
    let got = field_arguments(named);
    let want = BTreeMap::from([
        ("positional".to_string(), (Shape::Required, false)),
        ("optional".to_string(), (Shape::Optional, false)),
        ("flag".to_string(), (Shape::Flag, true)),
        ("named".to_string(), (Shape::Required, true)),
        ("plain".to_string(), (Shape::Required, false)),
        ("many".to_string(), (Shape::Optional, true)),
    ]);
    assert_eq!(got, want, "argument-shape reading drift");
}
