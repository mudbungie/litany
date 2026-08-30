//! The shipped `bash` definition is what the model is TOLD the shell is
//! (ARCH §3.3): the `SKILL.md` frontmatter `description` becomes the
//! wire `tools[].description` and `schemas/tools/bash.json` becomes the
//! wire `input_schema`, both verbatim
//! (`src/prompt/dispatch/tools.rs::compose`). Nothing downstream can
//! correct them, so the claims below are pinned here.
//!
//! Why these four claims and not prose taste (bl-298c): a gpt-5.x agent
//! read the older wording — *"Run a shell command via `sh -c` and return
//! its stdout"* — as a **remote interactive shell**. Asked for the
//! user's IP it answered *"the shell available to me would only show the
//! server's IP, not yours"*, then *"it runs on the remote agent
//! environment—not on your device"*, and after being told outright that
//! the tool was local it ran `curl` and still hedged: *"If this Bash
//! tool truly runs on your local device/network, that is your
//! public-facing IP. Otherwise, it belongs to the tool's execution
//! environment."* Every hedge is a fact the definition never stated.
//!
//! Codex — the harness these models are tuned against — never shows the
//! failure, and its *description* is not why: `shell_command` reads only
//! "Runs a shell command and returns its output." It answers the
//! question somewhere else, by injecting an `<environment_context>`
//! block (cwd, shell, date, timezone) and a sandbox-mode developer
//! message ahead of the request. litany injects no such frame — a
//! branch's assembled body is its goal, soul and transcript, all
//! operator-authored (§5.1) — so the tool definition is the only place
//! left that can state it, and it must: **local**, **non-interactive**,
//! **rooted in the agent's current working directory** — its worktree
//! until the agent moves with the `cd` tool (§3.3 *Working directory*),
//! and only worktree writes are committed — **stateless between tool
//! calls**.

/// The frontmatter `description` the shipped `<name>` skill ships —
/// byte-for-byte the string that reaches the wire.
fn wire_description(name: &str) -> String {
    let raw = super::SKILLS
        .get_file(format!("{name}/SKILL.md"))
        .expect("the skill pool ships this skill")
        .contents_utf8()
        .expect("SKILL.md is UTF-8");
    let body = crate::skill::frontmatter_yaml(raw).expect("SKILL.md has frontmatter");
    crate::skill::parse(body)
        .expect("frontmatter parses")
        .description
}

/// The shipped schema for `<name>`, parsed.
fn wire_schema(name: &str) -> serde_json::Value {
    let raw = super::TOOLS
        .get_file(format!("{name}.json"))
        .expect("the tool pool ships this schema")
        .contents();
    serde_json::from_slice(raw).expect("the shipped schema is JSON")
}

/// A claim is pinned by the phrase that makes it, so rewording that
/// drops the claim fails here rather than silently regressing.
fn asserts(text: &str, phrases: &[&str], what: &str) {
    for phrase in phrases {
        assert!(
            text.contains(phrase),
            "the {what} no longer says {phrase:?} (bl-298c) — it reads:\n{text}"
        );
    }
}

#[test]
fn the_bash_description_says_the_shell_is_local() {
    // The vision-kiwi failure exactly: "server", "remote", "your
    // device" are the model's own words for a gap this must close.
    asserts(
        &wire_description("bash"),
        &[
            "The shell is local",
            "on that same machine",
            "no server, container, or remote sandbox",
        ],
        "bash tool description",
    );
}

#[test]
fn the_bash_description_says_the_shell_is_non_interactive() {
    asserts(
        &wire_description("bash"),
        &["not an interactive terminal", "stdin closed and no TTY"],
        "bash tool description",
    );
}

#[test]
fn the_bash_description_says_where_it_runs_and_what_survives() {
    asserts(
        &wire_description("bash"),
        &[
            "Shell state does not carry over",
            "use the `cd` tool to move for real",
            "your worktree unless you moved it with the `cd` tool",
            "what you write outside it is not",
        ],
        "bash tool description",
    );
}

/// The §3.3 *result envelope* (bl-ffc5) is a promise about the wire, so
/// the description must make it: a model told only `is_error` cannot
/// tell exit 1 from exit 127 from the cancel's 143, and stderr on a
/// zero exit — the compiler warning, the deprecation notice — used to
/// be dropped entirely, leaving `2>&1` as the only way to see it. Codex
/// states the code in the content for the same reason, and gpt-5.x is
/// tuned to read it there. What the description promises here is what
/// `prompt::tool::envelope::render` delivers; the two are pinned apart
/// on purpose, so a change to either without the other fails.
#[test]
fn the_bash_description_promises_the_result_envelope() {
    asserts(
        &wire_description("bash"),
        &[
            "`Exit code: N` line",
            "`--- stderr ---` marker",
            "on success as well as failure",
        ],
        "bash tool description",
    );
}

#[test]
fn the_bash_schema_keeps_the_one_string_command_and_repeats_the_contract() {
    // Schema shape is deliberately unchanged (bl-298c). Codex — the
    // harness gpt-5.x is tuned against — declares `shell_command` with
    // `command` as a *single string* ("Shell script to run in the
    // user's default shell"), not the argv array its retired `shell`
    // tool used, so litany's one-string shape is already the shape
    // those models see. Its extra params are ones litany cannot honour:
    // `workdir` has no litany meaning because the cwd is not a
    // per-tool-call parameter at all — it is one mutable fact about the
    // agent, moved by an explicit `cd` tool call and read at every spawn
    // (§3.3 *Working directory*), so a per-tool-call override would be a
    // second home for it.
    // `timeout_ms` has no implementation — the executor imposes no
    // wall-clock limit. The executor's `Input` struct
    // (`prompt::tool::builtin::bash`) is `deny_unknown_fields`, so a
    // schema that grew either would be a promise the tool refuses at
    // runtime. The prose was the defect, not the shape.
    let schema = wire_schema("bash");
    let props = &schema["properties"];
    assert_eq!(props["command"]["type"], "string");
    assert_eq!(schema["required"], serde_json::json!(["command"]));
    assert_eq!(schema["additionalProperties"], serde_json::json!(false));
    assert_eq!(
        props.as_object().expect("properties is an object").len(),
        1,
        "bash takes one input; a second would need executor support"
    );
    asserts(
        props["command"]["description"]
            .as_str()
            .expect("command carries a description"),
        &[
            "on this machine",
            "runs non-interactively",
            "stdin is /dev/null",
            "no TTY",
            "starts in your current working directory",
            "discarded when it exits",
            "states the exit code on its first line",
            "`--- stderr ---` marker",
        ],
        "bash command-property description",
    );
}

/// The `dispatch` schema teaches `name` as an exposed parameter (§2.3,
/// yog bl-aca4): what a name buys (a `message`-addressable,
/// tree-readable child — identity clarity in subagent trees), that
/// omission mints a valid one-word name, and that a supplied collision
/// with a living name is refused. `required` stays `[role, goal]` —
/// omission must stay lawful for the mint to have a case.
#[test]
fn the_dispatch_schema_teaches_the_name_parameter_and_the_mint_on_omission() {
    let schema = wire_schema("dispatch");
    let props = &schema["properties"];
    assert_eq!(props["name"]["type"], "string");
    assert_eq!(schema["required"], serde_json::json!(["role", "goal"]));
    assert_eq!(schema["additionalProperties"], serde_json::json!(false));
    asserts(
        props["name"]["description"]
            .as_str()
            .expect("name carries a description"),
        &[
            "one unbroken word",
            "unique among the workspace's living agents",
            "the child's identity in every surface",
            "what `message` accepts in place of the child's agent id",
            "keeps the identities and tasks in a subagent tree clear",
            "If you omit it, a valid two-word PascalCase name is minted automatically",
            "already worn by a living agent is refused and no child is created",
        ],
        "dispatch name-property description",
    );
}

/// The `dispatch` schema must not teach that a subagent holds fewer
/// powers than its dispatcher (`docs/TAXONOMY.md`, "an agent is an
/// agent"; bl-a4d5). Three claims drifted and are pinned here. The
/// `goal` description once promised "the terminal **compacted**
/// result" — terminal compaction was deleted (ARCH §2.7 bl-9dbd note)
/// and what returns is the child's own terminal response — and it was
/// silent on the child being addressable mid-flight, which reads as a
/// one-shot fork. The `role` description once said "v0.4 Phase 2
/// supports `worker`", understating an open role set (ARCH §4.3;
/// `prompt::role::validate` enumerates no names).
#[test]
fn the_dispatch_schema_teaches_a_subagent_as_an_ordinary_agent() {
    let props = &wire_schema("dispatch")["properties"];
    let goal = props["goal"]
        .as_object()
        .and_then(|g| g["description"].as_str().map(str::to_owned))
        .expect("goal carries a description");
    asserts(
        &goal,
        &[
            "its terminal response",
            "`message` reaches it at any step boundary while it runs",
            "may dispatch subagents of its own",
        ],
        "dispatch goal-property description",
    );
    assert!(
        !goal.contains("compacted"),
        "goal must not promise a compacted result — terminal compaction is deleted: {goal}"
    );
    asserts(
        props["role"]["description"]
            .as_str()
            .expect("role carries a description"),
        &["The role set is open", "enumerates no role names"],
        "dispatch role-property description",
    );
}
