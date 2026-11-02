//! The argv surface of the two **per-agent mark verbs** — `workflow`
//! (ARCH §6 *The workflow mark*) and `model` (§4.3 *The model mark*) —
//! split from [`super::surface`] at the per-file cap along the seam the
//! verbs themselves share: naming a target writes, `--clear` unmarks,
//! bare reads, and the two writes are exclusive.

use crate::cmd::{Cli, Command};
use clap::Parser;
use std::path::PathBuf;

fn parse(args: &[&str]) -> Command {
    Cli::try_parse_from(args).expect("parses").command
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
fn model_parses_a_pick_a_clear_and_a_bare_read() {
    let Command::Model(a) = parse(&[
        "litany",
        "model",
        "/ws",
        "ag",
        "openai",
        "gpt-5",
        "--effort",
        "high",
        "--priority",
    ]) else {
        panic!()
    };
    assert_eq!(a.workspace, PathBuf::from("/ws"));
    assert_eq!(
        (a.agent.as_str(), a.provider.as_deref(), a.model.as_deref()),
        ("ag", Some("openai"), Some("gpt-5"))
    );
    assert_eq!(a.effort, Some(crate::config::Effort::High));
    assert!(a.priority && !a.clear);
    let Command::Model(b) = parse(&["litany", "model", "/ws", "ag", "--clear"]) else {
        panic!()
    };
    assert!(b.provider.is_none() && b.model.is_none() && b.clear);
    let Command::Model(c) = parse(&["litany", "model", "/ws", "ag"]) else {
        panic!()
    };
    assert!(c.provider.is_none() && !c.clear, "bare is the read");
    // A pick is a pair: a provider with no model is half a binding.
    assert!(Cli::try_parse_from(["litany", "model", "/ws", "ag", "openai"]).is_err());
    // The knobs ride a pick, never a clear or a read.
    assert!(Cli::try_parse_from(["litany", "model", "/ws", "ag", "--effort", "low"]).is_err());
    // A pick and a clear are exclusive.
    assert!(
        Cli::try_parse_from(["litany", "model", "/ws", "ag", "openai", "gpt-5", "--clear"])
            .is_err()
    );
    // The level is the closed vocabulary.
    assert!(
        Cli::try_parse_from([
            "litany", "model", "/ws", "ag", "openai", "gpt-5", "--effort", "max"
        ])
        .is_err()
    );
}
