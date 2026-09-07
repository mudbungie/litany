//! The census's **product**: the column-aligned table `litany skills`
//! prints (`docs/DESIGN_LEARNING_LOOP.md` §5).
//!
//! Split from the derivation beside it because they answer different
//! questions and neither reads the other's internals: [`super::census`]
//! decides what is true of a skill, this decides how a reader sees it.
//! The vocabulary of the columns — the two label sets and the rendering
//! of an absent date — lives here with the table that prints it.

use super::{Owner, Row, Stamp};

/// Rendered in place of an absent date — a skill nothing has loaded, or
/// a pool skill no config commit has ever touched (the pool is the
/// install's, so the lineage patches it never).
pub(super) const ABSENT: &str = "-";
/// The census table's column headings, printed even over no rows: a
/// workspace with no skills is the general path with empty inputs.
const HEADERS: [&str; 5] = ["SKILL", "OWNER", "STATE", "LAST USE", "LAST PATCH"];

impl Owner {
    pub(super) fn label(&self) -> &'static str {
        match self {
            Owner::Pool => "pool",
            Owner::Workspace => "workspace",
        }
    }
}

impl super::State {
    pub(super) fn label(&self) -> &'static str {
        match self {
            super::State::Active => "active",
            super::State::Claimed => "claimed",
            super::State::Unused => "unused",
            super::State::Archived => "archived",
        }
    }
}

/// The verb's one product: a column-aligned table, headers always.
pub(crate) fn render(rows: &[Row]) -> String {
    let cells: Vec<[String; 5]> = rows
        .iter()
        .map(|r| {
            [
                r.name.clone(),
                r.owner.label().to_owned(),
                r.state.label().to_owned(),
                age(r.last_use.as_ref()),
                age(r.last_patch.as_ref()),
            ]
        })
        .collect();
    let mut widths = HEADERS.map(str::len);
    for row in &cells {
        for (w, cell) in widths.iter_mut().zip(row) {
            *w = (*w).max(cell.len());
        }
    }
    let head = HEADERS.map(str::to_owned);
    std::iter::once(&head)
        .chain(cells.iter())
        .map(|row| line(row, &widths))
        .collect::<Vec<String>>()
        .join("\n")
}

/// One padded row, trailing whitespace trimmed off the last column.
fn line(row: &[String; 5], widths: &[usize; 5]) -> String {
    row.iter()
        .zip(widths.iter().copied())
        .map(|(cell, w)| format!("{cell:<w$}"))
        .collect::<Vec<String>>()
        .join("  ")
        .trim_end()
        .to_owned()
}

/// A date's rendering: git's own relative age, or [`ABSENT`].
fn age(stamp: Option<&Stamp>) -> String {
    stamp.map_or_else(|| ABSENT.to_owned(), |s| s.age.clone())
}
