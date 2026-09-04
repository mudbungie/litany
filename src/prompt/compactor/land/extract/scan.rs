//! What counts as a **reference** in one transcript entry leaving context
//! (`docs/DESIGN_CONTEXT_ECONOMY.md` §5.3) — the deterministic half of
//! the extract, a pure function of one entry's path and bytes.
//!
//! [`super`] assembles, deduplicates and bounds; this module only reads
//! an entry and names what is in it. No model, no tokenizer, no I/O.

use crate::prompt::dispatch::{MESSAGES_DIR, entry};
use crate::prompt::inbox::USER_SENDER;
use brazen::Content;

/// The references one entry carries, in the extract's section order.
#[derive(Default)]
pub(super) struct Refs {
    /// The entry's whole body when it is a user message — the review's
    /// "verbatim user messages" ([`is_user_message`]).
    pub(super) user_message: Option<String>,
    pub(super) errors: Vec<String>,
    pub(super) prs: Vec<String>,
    pub(super) shas: Vec<String>,
    pub(super) paths: Vec<String>,
}

/// Read one entry's references (module docs).
pub(super) fn of(path: &str, content: &str) -> Refs {
    // A `.json` entry's canonical blocks — the one home for where a
    // committed entry's content lives (§2.3, [`entry::blocks`]). A `.md`
    // entry (a delivered message, §2.11) has none and reads as its own
    // text.
    let blocks = if path.ends_with(".json") {
        entry::blocks(content.as_bytes())
    } else {
        Vec::new()
    };
    let mut refs = Refs {
        user_message: (is_user_message(path) && !content.trim().is_empty())
            .then(|| content.trim().to_string()),
        errors: error_lines(&blocks),
        ..Refs::default()
    };
    let text = if blocks.is_empty() {
        content.to_string()
    } else {
        blocks.iter().map(block_text).collect::<Vec<_>>().join("\n")
    };
    for token in text.split(delimiter).map(|t| t.trim_end_matches('.')) {
        if let Some(n) = pull_request(token) {
            refs.prs.push(format!("#{n}"));
        } else if is_sha(token) {
            refs.shas.push(token.to_string());
        } else if is_path(token) {
            refs.paths.push(token.to_string());
        }
    }
    refs
}

/// One block's text. A `tool_use`'s input rides as its JSON, which is
/// where a path a tool acted on is stated; the opaque provider-executed
/// blocks and the binary sources carry no reference and contribute
/// nothing.
fn block_text(block: &Content) -> String {
    match block {
        Content::Text(t) | Content::Thinking { text: t, .. } => t.clone(),
        Content::ToolUse { input, .. } => input.to_string(),
        Content::ToolResult { content, .. } => content
            .iter()
            .map(block_text)
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// The last line of every `is_error` tool result in `blocks`. A non-zero
/// exit is exactly what makes a result an error
/// ([`crate::prompt::tool::ToolOutcome`]: `is_error` is `false` for exit
/// 0), so the review's "error strings" and "non-zero exit tails" are one
/// class read once — and the *last* line, because that is where a failing
/// command states why.
fn error_lines(blocks: &[Content]) -> Vec<String> {
    blocks
        .iter()
        .filter(|b| matches!(b, Content::ToolResult { is_error: true, .. }))
        .filter_map(|b| {
            block_text(b)
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .map(|l| l.trim().to_string())
        })
        .collect()
}

/// `messages/<NNN>-user.md` — the only origin token that names the
/// operator ([`USER_SENDER`], §2.3 *Origins*).
fn is_user_message(path: &str) -> bool {
    path.starts_with(MESSAGES_DIR) && path.ends_with(&format!("-{USER_SENDER}.md"))
}

/// Token boundaries: whitespace, plus the punctuation that wraps a
/// reference in prose and the structure that wraps one in a `tool_use`'s
/// JSON — so `(src/main.rs)` and `{"path":"src/main.rs"}` both yield the
/// path they name. A trailing `.` is trimmed after the split rather than
/// being a boundary (`see #12.` is a reference, `a.rs` is not two), and
/// `:` and `/` are never boundaries, which is what keeps a URL one token
/// and therefore not a path ([`is_path`]).
fn delimiter(c: char) -> bool {
    c.is_whitespace() || "\"'`,;()[]{}<>*|=".contains(c)
}

/// The pull-request number in `token` — `#123`, or the `pull/123` a forge
/// URL carries — or `None`.
fn pull_request(token: &str) -> Option<&str> {
    let rest = match token.strip_prefix('#') {
        Some(rest) => rest,
        None => token.split("pull/").nth(1)?,
    };
    let digits = rest.split(|c: char| !c.is_ascii_digit()).next()?;
    (!digits.is_empty()).then_some(digits)
}

/// Is `token` a commit sha — 7 to 40 lowercase hex characters? Both a
/// digit and an `a`–`f` letter are required, which costs nothing real (a
/// sha of either shape is vanishingly unlikely) and keeps a date stamp
/// and an ordinary hex-lettered word out of the section.
fn is_sha(token: &str) -> bool {
    (7..=40).contains(&token.len())
        && token
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
        && token.chars().any(|c| c.is_ascii_digit())
        && token.chars().any(|c| c.is_ascii_alphabetic())
}

/// Is `token` a path — slash-separated, with an extension on its last
/// segment? A URL is slash-separated too and is not a path, so a scheme
/// disqualifies it.
fn is_path(token: &str) -> bool {
    if !token.contains('/') || token.contains("://") {
        return false;
    }
    token
        .rsplit('/')
        .next()
        .and_then(|last| last.rsplit_once('.'))
        .is_some_and(|(stem, ext)| {
            !stem.is_empty() && !ext.is_empty() && ext.chars().all(|c| c.is_ascii_alphanumeric())
        })
}
