//! Paging a cut capture, end to end (ARCH §3.3 *Paging a cut capture*,
//! `docs/DESIGN_CONTEXT_ECONOMY.md` §7).
//!
//! The unit tests either side of this one own their halves: the marker's
//! wording is `super::super::bound`'s, the grammar and every decline are
//! `builtin::read_tool_output`'s. What only a full chain can show is the
//! two ends meeting — that the address the executor MINTED into a cut
//! result is the address the `litany` binary REDEEMS against the record
//! that same call landed, with no store between them.
//!
//! The last test is the contract's other half, and it is the one that
//! would fail if someone ever made the recovery cheap by reading the
//! capture during assembly: with the whole `steps/` tree deleted, the
//! agent's wire history still composes, marker and address included.
//! The address lives in the committed transcript; the bytes it points at
//! do not have to exist for context to assemble (§5.1).

use super::fixtures::{FixedClock, HarnessRoot, StepDir, after_header};
use crate::config::ToolOutputBound;
use crate::prompt::clock::SystemClock;
use crate::prompt::dispatch::assembler::assemble;
use crate::prompt::tool::spawn::PathLookup;
use crate::prompt::tool::{SpawnTool, ToolCall, ToolExecutor};
use brazen::{Content, Role};
use serde_json::json;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

/// Forces the §3.3 second hop to miss so `read_tool_output` resolves at
/// the third — the cargo-built `litany` binary, the real subprocess.
struct NoPath;
impl PathLookup for NoPath {
    fn which_on_path(&self, _prefixed_name: &str) -> Option<PathBuf> {
        None
    }
}

fn bound(head: usize, tail: usize) -> Option<ToolOutputBound> {
    Some(ToolOutputBound {
        head_bytes: head,
        tail_bytes: tail,
    })
}

/// Pull the continuation address out of a cut result the way a model
/// does: it is the last token of the marker's recovery clause.
fn address_in(content: &str) -> String {
    let clause = content
        .split("read the cut middle with read_tool_output, address ")
        .nth(1)
        .expect("a cut result names its recovery");
    clause
        .split_whitespace()
        .next()
        .expect("the clause names an address")
        .to_string()
}

/// The whole loop: a tool whose stdout is cut, the address it minted,
/// and the page that address returns through the real binary.
#[test]
fn the_address_a_cut_minted_pages_the_bytes_the_cut_removed() {
    let root = HarnessRoot::new();
    // 78 lines of 100 bytes: more than one page, so the first page's
    // own continuation is observable rather than inferred.
    root.install(
        "chatty",
        r#"for r in 1 2 3; do for c in {a..z}; do printf '%.0s'"$c" {1..99}; printf '\n'; done; done"#,
    );
    let clock = FixedClock::default();
    let step = StepDir::new();
    let litany = crate::test_support::litany_binary();
    let exec = SpawnTool::new(root.path(), &clock, &litany).with_path_lookup(Box::new(NoPath));
    let cut = exec
        .execute(
            ToolCall {
                id: "toolu_big",
                name: "chatty",
                input: &json!({}),
            },
            &step.path,
            &AtomicBool::new(false),
            bound(40, 40),
        )
        .expect("the tool runs");
    let cut_text = String::from_utf8(cut.content).expect("ASCII fixture");
    let address = address_in(&cut_text);
    assert_eq!(
        address,
        "steps/convid/001/tools/toolu_big/output.json#stdout@40"
    );

    // Redeem it as the agent itself would: one more tool call, resolved
    // at the third hop into the real `litany tool read_tool_output`.
    let clock = SystemClock;
    let litany = crate::test_support::litany_binary();
    let exec = SpawnTool::new(root.path(), &clock, &litany).with_path_lookup(Box::new(NoPath));
    let page = exec
        .execute(
            ToolCall {
                id: "toolu_page",
                name: "read_tool_output",
                input: &json!({ "address": address }),
            },
            &step.path,
            &AtomicBool::new(false),
            None,
        )
        .expect("the page reads");
    assert!(
        !page.is_error,
        "{:?}",
        String::from_utf8_lossy(&page.content)
    );
    let body = String::from_utf8(after_header(&page.content).to_vec()).expect("ASCII");
    // The page opens at byte 40 of the capture — inside the first line,
    // which the cut showed only the first 40 bytes of.
    assert!(body.starts_with(&"a".repeat(59)), "{body}");
    // And it says where it stopped, in `read_file`'s voice, on stderr.
    assert!(
        body.contains(
            "--- stderr ---\nread_tool_output: bytes 40-4136 of 7800 from \
             steps/convid/001/tools/toolu_big/output.json stdout; continue with address \
             steps/convid/001/tools/toolu_big/output.json#stdout@4136\n"
        ),
        "{body}"
    );
}

/// An address naming another agent is refused through the whole chain,
/// by name — the domain bound is not a claim the executor makes, it is
/// a segment of the address the tool compares.
#[test]
fn a_foreign_agent_address_is_refused_through_the_binary() {
    let root = HarnessRoot::new();
    let clock = SystemClock;
    let step = StepDir::new();
    let litany = crate::test_support::litany_binary();
    let exec = SpawnTool::new(root.path(), &clock, &litany).with_path_lookup(Box::new(NoPath));
    let out = exec
        .execute(
            ToolCall {
                id: "toolu_theft",
                name: "read_tool_output",
                input: &json!({
                    "address": "steps/someone-else/001/tools/toolu_1/output.json#stdout@0"
                }),
            },
            &step.path,
            &AtomicBool::new(false),
            None,
        )
        .expect("the tool runs");
    assert!(out.is_error);
    let text = String::from_utf8(out.content).expect("ASCII");
    assert!(
        text.contains(
            "address names agent \"someone-else\", and you are \"convid\": \
             an agent may page only its own tool captures"
        ),
        "{text}"
    );
}

/// Nothing under `steps/` is read on the assembly path (§2.3
/// *Diagnostic-only contract*, §5.1). The cut result's transcript entry
/// carries the marker and the address; the capture they point at is
/// deleted, and the wire history composes unchanged.
#[test]
fn assembly_reads_nothing_under_steps() {
    let root = HarnessRoot::new();
    root.install(
        "chatty",
        r#"printf 'HEAD%s TAIL\n' "$(printf 'm%.0s' {1..200})""#,
    );
    let clock = FixedClock::default();
    let step = StepDir::new();
    let litany = crate::test_support::litany_binary();
    let exec = SpawnTool::new(root.path(), &clock, &litany).with_path_lookup(Box::new(NoPath));
    let cut = exec
        .execute(
            ToolCall {
                id: "toolu_big",
                name: "chatty",
                input: &json!({}),
            },
            &step.path,
            &AtomicBool::new(false),
            bound(8, 8),
        )
        .expect("the tool runs");
    let text = String::from_utf8(cut.content).expect("ASCII fixture");
    let address = address_in(&text);

    // Commit it as the transcript entry the executor would (§2.3).
    let messages = step.worktree.join("messages");
    std::fs::create_dir_all(&messages).expect("mkdir messages");
    std::fs::write(
        messages.join("001-tool.json"),
        serde_json::to_vec(&[Content::ToolResult {
            tool_use_id: "toolu_big".into(),
            content: vec![Content::Text(text.clone())],
            is_error: false,
        }])
        .expect("blocks serialize"),
    )
    .expect("write entry");

    // Now take the whole diagnostic tree away.
    let steps_root = step
        .path
        .parent()
        .and_then(std::path::Path::parent)
        .expect("steps/");
    std::fs::remove_dir_all(steps_root).expect("remove steps/");
    assert!(!steps_root.exists());

    let msgs = assemble(&step.worktree, None).expect("assembly needs no step record");
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].role, Role::Tool);
    let Some(Content::ToolResult { content, .. }) = msgs[0].content.first() else {
        panic!("the entry composes as a tool result");
    };
    let Some(Content::Text(composed)) = content.first() else {
        panic!("the result carries its text");
    };
    assert!(composed.contains(&address), "{composed}");
    assert!(
        composed.contains("read the cut middle with read_tool_output"),
        "{composed}"
    );
}
