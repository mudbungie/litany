//! SKILL.md frontmatter — the shared shape the descriptions-always
//! producer snapshots into `descriptions/skills/<name>.md` (ARCH §3.3
//! *Description-always*) and the tools composer reads back to fill a
//! tool entry's `description` (§3.3 point 3). Keeping the format in one
//! module is what stops producer and consumer drifting: the producer
//! extracts a SKILL.md's frontmatter body with [`frontmatter_yaml`] and
//! writes it verbatim; the consumer parses that stored body with
//! [`parse`]. One home for the fact (`docs/PRINCIPLES.md`, single source
//! of truth).

use serde::Deserialize;

/// The YAML frontmatter fence line used by SKILL.md files.
const FENCE: &str = "---";

/// Typed view of the `name` + `description` a SKILL.md frontmatter
/// declares (ARCH §3.3). Extra keys are tolerated — only these two are
/// load-bearing (the `description` becomes the tool entry's
/// `description`; the `name` is retained for provenance).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Frontmatter {
    /// The skill name (matches the directory it lives under).
    pub name: String,
    /// Progressive-disclosure blurb — when and how to use the skill.
    pub description: String,
}

/// The YAML body of a SKILL.md's leading `---` … `---` frontmatter
/// block, fences excluded, returned as a borrowed slice. `None` when the
/// text does not open with a `---` fence line or the block is never
/// closed. This is what the producer writes verbatim into
/// `descriptions/skills/<name>.md`.
pub fn frontmatter_yaml(md: &str) -> Option<&str> {
    let open = format!("{FENCE}\n");
    let after_open = md.strip_prefix(&open)?;
    for (i, _) in after_open.match_indices(FENCE) {
        let at_line_start = i == 0 || after_open.as_bytes()[i - 1] == b'\n';
        let after = &after_open[i + FENCE.len()..];
        let closes_line = after.is_empty() || after.starts_with('\n');
        if at_line_start && closes_line {
            return Some(&after_open[..i]);
        }
    }
    None
}

/// Parse a stored `descriptions/skills/<name>.md` — the frontmatter YAML
/// body the producer wrote — into its typed fields.
pub fn parse(yaml_body: &str) -> Result<Frontmatter, serde_yaml_ng::Error> {
    serde_yaml_ng::from_str(yaml_body)
}

/// The `litany skills` derivation (`docs/DESIGN_LEARNING_LOOP.md` §5) —
/// the census of both skill homes, derived from git and stored nowhere.
pub(crate) mod census;

#[cfg(test)]
mod tests;
