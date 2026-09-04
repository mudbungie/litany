//! The argv surface and binding types: `Cli` round-trips for every verb
//! (byte-for-byte parse parity, §3.4), the [`Error`] Display shape, the
//! [`Outcome`] handoff mapping, and the [`prelude`] re-exports.

use crate::cmd::{BUILTIN_TOOLS, Cli, Command, Error, Outcome, advance, prelude};
use crate::prompt::dispatch::advance::cli::AdvanceHandoff;
use clap::{CommandFactory, Parser};
use std::path::PathBuf;

fn parse(args: &[&str]) -> Command {
    Cli::try_parse_from(args).expect("parses").command
}

#[test]
fn new_parses_optional_path() {
    assert!(matches!(parse(&["litany", "new"]), Command::New(a) if a.path.is_none()));
    let Command::New(a) = parse(&["litany", "new", "/w"]) else {
        panic!()
    };
    assert_eq!(a.path.unwrap(), PathBuf::from("/w"));
}

#[test]
fn config_parses_name_and_flags() {
    let Command::Config(a) = parse(&[
        "litany", "config", "/ws", "strict", "--from", "src", "--orphan",
    ]) else {
        panic!()
    };
    assert_eq!(a.workspace, PathBuf::from("/ws"));
    assert_eq!(a.name.unwrap(), "strict");
    assert_eq!(a.from.unwrap(), "src");
    assert!(a.orphan);
    // Minimal form: bare workspace, defaults elsewhere.
    let Command::Config(b) = parse(&["litany", "config", "/ws"]) else {
        panic!()
    };
    assert!(b.name.is_none() && b.from.is_none() && !b.orphan);
}

#[test]
fn prompt_parses_repo_and_message() {
    let Command::Prompt(a) = parse(&["litany", "prompt", "/r", "hello there"]) else {
        panic!()
    };
    assert_eq!(a.repo, PathBuf::from("/r"));
    assert_eq!(a.message, "hello there");
}

#[test]
fn dispatch_parses_positional_and_goal() {
    let Command::Dispatch(a) = parse(&[
        "litany", "dispatch", "worker", "/r", "br", "--goal", "do it",
    ]) else {
        panic!()
    };
    assert_eq!((a.role.as_str(), a.branch.as_str()), ("worker", "br"));
    assert_eq!(a.goal.unwrap(), "do it");
    let Command::Dispatch(b) = parse(&["litany", "dispatch", "compactor", "/r", "br"]) else {
        panic!()
    };
    assert!(b.goal.is_none());
}

#[test]
fn stop_parses_stop_children_flag() {
    let Command::Stop(a) = parse(&["litany", "stop", "/r", "br", "--stop-children"]) else {
        panic!()
    };
    assert!(a.stop_children);
    let Command::Stop(b) = parse(&["litany", "stop", "/r", "br"]) else {
        panic!()
    };
    assert!(!b.stop_children);
}

#[test]
fn message_scan_bundle_replay_advance_parse() {
    let Command::Message(m) = parse(&["litany", "message", "/ws", "ag", "hi"]) else {
        panic!()
    };
    assert_eq!((m.agent.as_str(), m.content.as_str()), ("ag", "hi"));
    let Command::Scan(s) = parse(&["litany", "scan", "/ws"]) else {
        panic!()
    };
    assert_eq!(s.workspace, PathBuf::from("/ws"));
    let Command::Bundle(b) = parse(&["litany", "bundle", "/ws", "ag", "/out"]) else {
        panic!()
    };
    assert_eq!(b.out_dir, PathBuf::from("/out"));
    let Command::Replay(rep) = parse(&["litany", "replay", "/a"]) else {
        panic!()
    };
    assert_eq!(rep.archive, PathBuf::from("/a"));
    let Command::Advance(v) = parse(&["litany", "advance", "/ws", "ag"]) else {
        panic!()
    };
    assert_eq!(
        (v.workspace, v.agent.as_str()),
        (PathBuf::from("/ws"), "ag")
    );
}

#[test]
fn workflow_parses_config_and_clear() {
    let Command::Workflow(a) = parse(&["litany", "workflow", "/ws", "ag", "--config", "alt"])
    else {
        panic!()
    };
    assert_eq!(a.workspace, PathBuf::from("/ws"));
    assert_eq!((a.agent.as_str(), a.config.as_deref()), ("ag", Some("alt")));
    assert!(!a.clear);
    let Command::Workflow(b) = parse(&["litany", "workflow", "/ws", "ag", "--clear"]) else {
        panic!()
    };
    assert!(b.config.is_none() && b.clear);
    // The two modes are exclusive: `--config` marks, `--clear` unmarks.
    assert!(
        Cli::try_parse_from([
            "litany", "workflow", "/ws", "ag", "--config", "x", "--clear"
        ])
        .is_err()
    );
}

#[test]
fn tool_and_prime_parse() {
    assert!(matches!(parse(&["litany", "tool", "bash"]), Command::Tool(t) if t.name == "bash"));
    assert!(matches!(parse(&["litany", "prime"]), Command::Prime(_)));
}

#[test]
fn error_display_is_the_prefixed_stderr_shape() {
    assert_eq!(Error::new("new", "boom").to_string(), "litany new: boom");
    assert_eq!(
        Error::new(format!("dispatch {}", "worker"), "bad role").to_string(),
        "litany dispatch worker: bad role"
    );
    assert_eq!(
        Error::new(format!("tool {}", "bash"), "no such").to_string(),
        "litany tool bash: no such"
    );
    // Debug is derivable; exercise it so the derive is covered.
    assert!(format!("{:?}", Error::new("scan", "x")).contains("scan"));
}

#[test]
fn advance_outcome_maps_both_handoff_arms() {
    let exec = advance::outcome_of(AdvanceHandoff::Exec(std::process::Command::new("true")));
    assert!(matches!(exec, Outcome::Exec(_)));
    let done = advance::outcome_of(AdvanceHandoff::Done);
    assert!(matches!(done, Outcome::Quiet));
}

#[test]
fn noop_editor_is_a_silent_ok() {
    // The shared no-op `$EDITOR` hand-off is invoked directly (only the
    // `config` verb calls an editor, and it uses the writing one).
    super::noop_editor(std::path::Path::new("/unused")).unwrap();
}

#[test]
fn prelude_reexports_the_binding_mechanisms() {
    // The §3.4 binding preludes are re-exported here for the binding to
    // invoke; reference each so the seam is proven wired. `stop_flag` is
    // side-effect-free to read; the two effecting mechanisms are only
    // named (the binding calls them, tests must not mutate the runner's
    // pgid or signal disposition — those are exercised in `prompt::stop`).
    let _leader: fn() = prelude::become_pgid_leader;
    let _handler: fn() = prelude::install_stop_handler;
    let flag: &std::sync::atomic::AtomicBool = prelude::stop_flag();
    let _ = flag.load(std::sync::atomic::Ordering::SeqCst);
}

/// `litany --version` (ARCH §4.4 "Version skew is guarded") prints both
/// litany's own version and the linked brazen pin, and the two readers of
/// that one pin — [`crate::prompt::cli_version`] (beside the pin, named by
/// the clap attribute here) and the load-time guard's
/// [`crate::prompt::brazen_pin`] — must agree by construction: a
/// bijection, not a second hard-coded string.
#[test]
fn cli_version_pairs_litany_and_the_brazen_pin() {
    let v = crate::prompt::cli_version();
    assert!(v.starts_with(env!("CARGO_PKG_VERSION")), "{v}");
    let brazen = v
        .strip_prefix(env!("CARGO_PKG_VERSION"))
        .and_then(|rest| rest.strip_prefix(" (brazen "))
        .and_then(|rest| rest.strip_suffix(')'))
        .unwrap_or_else(|| panic!("unexpected cli_version shape: {v}"));
    assert_eq!(brazen, crate::prompt::brazen_pin());
    // Wired into the actual clap surface, not just computed and unused.
    assert_eq!(Cli::command().get_version(), Some(v));
}

/// Every verb's positionals render with help text. A blank `<NAME>` /
/// `<WORKSPACE>` is the CLI declining to answer a question the README
/// invites the user to ask of it; `advance` already documented its pair,
/// and this holds the rest to the same bar. The US-23 parity checker
/// asserts the argument *set*, so documenting them changes nothing it
/// reads.
#[test]
fn every_positional_argument_documents_itself() {
    let cli = Cli::command();
    for sub in cli.get_subcommands() {
        for arg in sub.get_positionals() {
            assert!(
                arg.get_help().is_some(),
                "{} <{}> renders blank in --help",
                sub.get_name(),
                arg.get_id()
            );
        }
    }
}

/// `litany tool --help` names the built-in pool, and names it from the
/// same [`builtin::NAMES`] the unknown-tool decline renders — one list,
/// two surfaces (PRINCIPLES single source of truth). The compactor pair
/// (§2.7) is routed but unadvertised: it is injected for the compactor
/// role, never a name to elect, so it must not leak into the help.
#[test]
fn tool_name_help_names_the_built_in_pool() {
    let cli = Cli::command();
    let tool = cli
        .get_subcommands()
        .find(|s| s.get_name() == "tool")
        .expect("tool verb");
    let arg = tool.get_positionals().next().expect("<NAME> positional");
    let help = arg.get_help().expect("help").to_string();
    for name in BUILTIN_TOOLS {
        assert!(help.contains(name), "{help}");
    }
    assert!(!help.contains("write_summary"), "{help}");
}

/// The pool is readable on the surface (ARCH §3.3): a host that installs
/// a `ToolInjection` routes every invocation itself, so it must be able
/// to *ask* which names this engine performs instead of restating them.
/// The export and the human render are the same list, so an eighth
/// built-in reaches a host without either side being edited.
#[test]
fn the_built_in_pool_is_readable_on_the_command_surface() {
    assert_eq!(
        BUILTIN_TOOLS.join(", "),
        crate::prompt::tool::builtin::pool()
    );
}

#[test]
fn preludes_are_named_per_verb_by_the_surface() {
    // The §2.9 prelude-per-verb map (ARCH §3.4 binding-preludes seam) is
    // a query on the surface, so both bindings read one fact instead of
    // each keeping a match in step with it. Compared as fn pointers —
    // naming them is safe; *calling* them would mutate the test runner's
    // pgid and signal disposition (covered in `prompt::stop`).
    let want =
        |c: Command| -> Vec<*const ()> { c.preludes().iter().map(|f| *f as *const ()).collect() };
    let leader = prelude::become_pgid_leader as *const ();
    let handler = prelude::install_stop_handler as *const ();

    // Driver verbs own a step loop: process group + stopped-deposit handler.
    for argv in [
        &["litany", "prompt", "/w", "hi"][..],
        &["litany", "advance", "/w", "20260101-a1"][..],
    ] {
        assert_eq!(want(parse(argv)), vec![leader, handler], "{argv:?}");
    }
    // `dispatch` is child re-entry — a group of its own, but it drives
    // nothing, so no handler.
    assert_eq!(
        want(parse(&[
            "litany", "dispatch", "worker", "/w", "b", "--goal", "g"
        ])),
        vec![leader],
    );
    // Every other verb needs neither.
    for argv in [
        &["litany", "new"][..],
        &["litany", "config", "/w"][..],
        &["litany", "retarget", "/w", "20260101-a1"][..],
        &["litany", "workflow", "/w", "20260101-a1"][..],
        &["litany", "stop", "/w", "20260101-a1"][..],
        &["litany", "message", "/w", "20260101-a1", "c"][..],
        &["litany", "scan", "/w"][..],
        &["litany", "skills", "/w"][..],
        &["litany", "bundle", "/w", "20260101-a1", "/out"][..],
        &["litany", "replay", "/b.bundle"][..],
        &["litany", "tool", "bash"][..],
        &["litany", "prime"][..],
    ] {
        assert!(parse(argv).preludes().is_empty(), "{argv:?}");
    }
}
