//! What the shipped `python` definition promises the model
//! (`docs/DESIGN_CODE_EXECUTION.md` §2.2, §2.4, §2.6). Same stake as
//! `bash`'s (see [`super::toolspec`]): the `SKILL.md` frontmatter
//! becomes the wire `tools[].description` and `schemas/tools/python.json`
//! the wire `input_schema`, both verbatim, and nothing downstream can
//! correct them.
//!
//! Five claims are pinned, one per fact a model cannot discover by
//! trying: the interpreter is **local** and assumed rather than probed
//! (§2.6, so a model does not hedge about a sandbox and an operator
//! reads why a missing `python3` is a grant question); a program's tool
//! calls go through the **generated stub module** and are ordinary
//! invocations; a failed tool is a **value, not an exception**, which is
//! what makes a fan-out worth writing; **only stdout returns** (§2.4 —
//! the operator's ruling, and the whole reason the tool saves context);
//! and **depth 1**.

use super::toolspec::{asserts, wire_description, wire_schema};

#[test]
fn the_python_description_says_the_interpreter_is_local_and_assumed() {
    asserts(
        &wire_description("python"),
        &[
            "The interpreter is local",
            "on that same machine",
            "`python3` resolved on that machine's PATH",
            "no server, container, or remote sandbox",
        ],
        "python tool description",
    );
}

#[test]
fn the_python_description_teaches_the_stub_module_and_the_result_shape() {
    asserts(
        &wire_description("python"),
        &[
            "`import litany_tools`",
            "one keyword-only function per tool you may call",
            "`litany_tools.invoke(name, arguments)`",
            "same permission, same review, same record",
            "`.stdout`, `.stderr`, `.exit_code` and `.ok`",
            "raises only when the harness cannot be reached",
        ],
        "python tool description",
    );
}

#[test]
fn the_python_description_says_only_stdout_returns_and_depth_is_one() {
    asserts(
        &wire_description("python"),
        &[
            "Only what your program prints on stdout comes back to you",
            "No inner tool result reaches you",
            "`python` itself may not be called from a program (depth 1)",
        ],
        "python tool description",
    );
}

#[test]
fn the_python_description_says_where_it_runs_and_what_it_costs() {
    asserts(
        &wire_description("python"),
        &[
            "One tool call runs exactly one program",
            "no time limit",
            "your worktree unless you moved it with the `cd` tool",
            "dozens of tool calls cost you one result to read",
        ],
        "python tool description",
    );
}

#[test]
fn the_python_schema_takes_one_string_program_and_repeats_the_contract() {
    // One input and no second knob: a `timeout` has no implementation
    // (§2.5 — the executor imposes no deadline and `litany stop` is the
    // bound), and a `cwd` would be a second home for the one mutable
    // per-agent fact the `cd` tool owns (ARCH §3.3). The built-in's
    // `Input` is `deny_unknown_fields`, so either would be a promise the
    // tool refuses at runtime.
    let schema = wire_schema("python");
    let props = &schema["properties"];
    assert_eq!(props["program"]["type"], "string");
    assert_eq!(schema["required"], serde_json::json!(["program"]));
    assert_eq!(schema["additionalProperties"], serde_json::json!(false));
    assert_eq!(
        props.as_object().expect("properties is an object").len(),
        1,
        "python takes one input; a second would need executor support"
    );
    asserts(
        props["program"]["description"]
            .as_str()
            .expect("program carries a description"),
        &[
            "fed to `python3 -` on this machine",
            "runs to completion with no time limit",
            "`import litany_tools`",
            "Only what the program prints on stdout comes back to you",
            "cannot be called from a program (depth 1)",
            "states the exit code on its first line",
            "`--- stderr ---` marker",
        ],
        "python program-property description",
    );
}
