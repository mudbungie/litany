//! The proposal listing's **one product**: the table `litany proposal`
//! prints (ARCH §3.4 one-product convention,
//! `docs/DESIGN_LEARNING_LOOP.md` §3).
//!
//! Rendering is separated from the queries that derive the rows
//! ([`super::ops`]) for the reason the skill census separates them: a
//! row is a fact about refs and a column is a decision about reading,
//! and only one of the two changes when a terminal gets wider.
//!
//! The headers always print — a workspace with no staged proposal
//! prints the headers and nothing else, the general path with empty
//! inputs rather than an "(none)" arm.

use super::Row;

/// The columns, in reading order: which proposal, against which
/// lineage, on which commit, whether that commit is still the head, how
/// big it is, and what the reviewer said it was.
const HEADERS: [&str; 6] = ["ID", "LINEAGE", "PARENT", "STATE", "DIFF", "SUBJECT"];

/// Render the rows as a padded table, headers included.
pub fn render(rows: &[Row]) -> String {
    let cells: Vec<[String; 6]> = rows.iter().map(cells).collect();
    let mut widths = HEADERS.map(str::len);
    for row in &cells {
        for (w, cell) in widths.iter_mut().zip(row) {
            *w = (*w).max(cell.chars().count());
        }
    }
    let mut out = line(&HEADERS.map(String::from), &widths);
    for row in &cells {
        out.push_str(&line(row, &widths));
    }
    out
}

/// One row's cells. **State is derived**, so it is rendered from the
/// derivation rather than stored beside it: `fresh` when the parent is
/// still a lineage head, `stale` when it is not.
fn cells(row: &Row) -> [String; 6] {
    [
        row.id.clone(),
        crate::name::pool(&row.lineages),
        row.parent.clone(),
        match row.fresh {
            true => "fresh".into(),
            false => "stale".into(),
        },
        row.diffstat.clone(),
        row.subject.clone(),
    ]
}

/// One padded line, trailing padding trimmed (a terminal's last column
/// is not a place to put spaces).
fn line(cells: &[String; 6], widths: &[usize; 6]) -> String {
    let mut out = String::new();
    for (i, (cell, width)) in cells.iter().zip(widths).enumerate() {
        if i + 1 == cells.len() {
            out.push_str(cell);
        } else {
            out.push_str(&format!("{cell:<width$}  "));
        }
    }
    out.push('\n');
    out.trim_end_matches(' ').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, lineages: &[&str], fresh: bool) -> Row {
        Row {
            id: id.into(),
            lineages: lineages.iter().map(|l| (*l).to_string()).collect(),
            parent: "9f2c1ab4de01".into(),
            fresh,
            diffstat: "1 file changed, 4 insertions(+)".into(),
            subject: "notes: record what the span taught".into(),
        }
    }

    #[test]
    fn the_state_column_reads_the_derivation_both_ways() {
        let table = render(&[row("a1-r1", &["default"], true), row("a1-r2", &[], false)]);
        let mut lines = table.lines();
        assert!(lines.next().unwrap().starts_with("ID "), "{table}");
        let fresh = lines.next().unwrap();
        assert!(
            fresh.contains("default") && fresh.contains("fresh"),
            "{fresh}"
        );
        let stale = lines.next().unwrap();
        // No lineage stands on its parent, which is the same fact the
        // state column states — rendered as the empty pool, not blank.
        assert!(
            stale.contains("(none)") && stale.contains("stale"),
            "{stale}"
        );
        assert_eq!(lines.next(), None, "one line per row and no more");
    }

    #[test]
    fn an_empty_listing_is_the_headers_and_nothing_else() {
        assert_eq!(render(&[]).lines().count(), 1);
    }

    #[test]
    fn columns_are_padded_to_the_widest_cell_and_the_last_is_not() {
        let table = render(&[row("a-very-long-reviewer-id", &["default"], true)]);
        let header = table.lines().next().unwrap();
        assert!(header.starts_with("ID                      "), "{header}");
        assert!(header.ends_with("SUBJECT"), "{header}");
    }
}
