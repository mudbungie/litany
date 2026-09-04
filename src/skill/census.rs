//! The **skill census** — the derivation behind `litany skills`
//! (`docs/DESIGN_LEARNING_LOOP.md` §5, "the curator is a query").
//!
//! No store, no counter, no curator process: every fact a row carries is
//! already a commit, and git already dates it. A skill's **last use** is
//! the newest commit on an `agents/*` branch that *added*
//! `skills/<name>/` — the `load_skill` copy is the use (ARCH §3.3) — and
//! its **last patch** is the newest `config/*` commit touching that path
//! in either of its two homes, `skills/<name>/` or the archive container
//! `skills/archived/<name>/`.
//!
//! **Two exclusions make the use query mean what it says.** The walk runs
//! `--branches=agents/*` so a deleted agent's history is gone with its
//! ref — "living branch" needs no second derivation — and `--not
//! --branches=config/*` so the config commit that *authored* a workspace
//! skill is not read as every descendant agent's election. What is left
//! is exactly the adds an agent branch made on its own.
//!
//! **Ages come from git** (`%cr`), not from a clock this module reads:
//! the age is what the reader wants and the horizon is the reader's
//! (§5 refuses a stored `stale` state). `%ct` rides beside it for the
//! ordering — oldest-used first, never-used first of all.

use crate::template::{GitRunner, descriptions};
use crate::workspace::{self, SKILLS_DIR};
use std::collections::BTreeSet;
use std::path::Path;

/// `git log`'s answer for one row: the sortable stamp and the rendered
/// age, taken in one walk so the two can never disagree.
const FORMAT: &str = "--format=%ct %cr";
/// Rendered in place of an absent date — a skill nothing has loaded, or
/// a pool skill no config commit has ever touched (the pool is the
/// install's, so the lineage patches it never).
const ABSENT: &str = "-";
/// The archive container's name, reserved in both homes.
const ARCHIVED: &str = descriptions::ARCHIVED_SUBDIR;
/// The census table's column headings, printed even over no rows: a
/// workspace with no skills is the general path with empty inputs.
const HEADERS: [&str; 5] = ["SKILL", "OWNER", "STATE", "LAST USE", "LAST PATCH"];

/// Which home holds the body — ownership is the path (ARCH §3.3).
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Owner {
    /// `<data-root>/skills/<name>/`, shared by every workspace on the box.
    Pool,
    /// `skills/<name>/` in the config lineage, versioned and forkable.
    Workspace,
}

/// The three states of §5. There is no `stale`: a wall-clock horizon is
/// policy, policy is config, and this verb adds none.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum State {
    /// A living `agents/*` branch has loaded it — or a tool claims it, so
    /// it composes as that tool's description without ever being loaded.
    Active,
    /// Neither: no living branch carries it and no tool claims it.
    Unused,
    /// `skills/archived/<name>/` in the followed config commit. Composes
    /// nowhere and cannot be named by `load_skill`.
    Archived,
}

/// One commit's date, twice: sortable seconds and git's own rendering.
#[derive(Debug)]
pub(crate) struct Stamp {
    secs: i64,
    age: String,
}

/// One census row.
#[derive(Debug)]
pub(crate) struct Row {
    pub(crate) name: String,
    pub(crate) owner: Owner,
    pub(crate) state: State,
    last_use: Option<Stamp>,
    last_patch: Option<Stamp>,
}

impl Owner {
    fn label(&self) -> &'static str {
        match self {
            Owner::Pool => "pool",
            Owner::Workspace => "workspace",
        }
    }
}

impl State {
    fn label(&self) -> &'static str {
        match self {
            State::Active => "active",
            State::Unused => "unused",
            State::Archived => "archived",
        }
    }
}

/// Every skill both homes offer, one row each, oldest-used first.
/// `commit` is the config commit the workspace skills and the archive
/// container are read from; `data_root` is the install root whose
/// `skills/` pool is the other home.
pub(crate) fn census(ws: &Path, commit: &str, data_root: &Path, git: &dyn GitRunner) -> Vec<Row> {
    let archived = tree_names(ws, commit, &format!("{SKILLS_DIR}/{ARCHIVED}"), git);
    let mut committed = tree_names(ws, commit, SKILLS_DIR, git);
    committed.remove(ARCHIVED);
    let pooled = pool_names(&data_root.join(descriptions::SKILLS_SUBDIR));
    let mut rows: Vec<Row> = archived
        .iter()
        .map(|n| (n, Owner::Workspace))
        .chain(committed.iter().map(|n| (n, Owner::Workspace)))
        .chain(pooled.iter().map(|n| (n, Owner::Pool)))
        .map(|(name, owner)| row(ws, commit, name, owner, &archived, git))
        .collect();
    order(&mut rows);
    rows
}

/// Oldest-used first, a never-used skill oldest of all (`None` precedes
/// `Some`), then by name so the table is stable across runs. Its own
/// function because a wall clock is a bad thing to prove an ordering
/// with: the fixture that constructs the stamps proves this, and the
/// fixtures that run real git prove where a stamp comes from.
fn order(rows: &mut [Row]) {
    rows.sort_by(|a, b| key(a).cmp(&key(b)));
}

/// One row's ordering key ([`order`]).
fn key(r: &Row) -> (Option<i64>, &str) {
    (r.last_use.as_ref().map(|s| s.secs), r.name.as_str())
}

/// Build one row: the two dates, then the state they and the tree decide.
fn row(
    ws: &Path,
    commit: &str,
    name: &str,
    owner: Owner,
    archived: &BTreeSet<String>,
    git: &dyn GitRunner,
) -> Row {
    let body = format!("{SKILLS_DIR}/{name}");
    let stored = format!("{SKILLS_DIR}/{ARCHIVED}/{name}");
    let last_use = stamp(
        ws,
        git,
        &[
            "log",
            "--diff-filter=A",
            "--max-count=1",
            FORMAT,
            "--branches=agents/*",
            "--not",
            "--branches=config/*",
            "--",
            &body,
        ],
    );
    let last_patch = stamp(
        ws,
        git,
        &[
            "log",
            "--max-count=1",
            FORMAT,
            "--branches=config/*",
            "--",
            &body,
            &stored,
        ],
    );
    let state = if archived.contains(name) {
        State::Archived
    } else if last_use.is_some() || tool_claimed(ws, commit, name, git) {
        State::Active
    } else {
        State::Unused
    };
    Row {
        name: name.to_owned(),
        owner,
        state,
        last_use,
        last_patch,
    }
}

/// Is a tool schema committed beside the skill's description? A claimed
/// skill composes as that tool's `description` on every model call
/// (ARCH §3.3), so it is never idle and §5 exempts it from `unused`.
fn tool_claimed(ws: &Path, commit: &str, name: &str, git: &dyn GitRunner) -> bool {
    let path = format!(
        "{}/{}/{name}.json",
        descriptions::DESCRIPTIONS_DIR,
        descriptions::TOOLS_SUBDIR
    );
    workspace::control_exists(ws, commit, &path, git)
}

/// One `git log` walk's newest answer, or `None` when the walk names no
/// commit — an unreadable ref set, an empty workspace and a path no
/// commit ever carried are one answer, not three.
fn stamp(ws: &Path, git: &dyn GitRunner, args: &[&str]) -> Option<Stamp> {
    let out = git
        .run_capture(&workspace::repo_git(ws), args)
        .unwrap_or_default();
    let (secs, age) = out.lines().next()?.trim().split_once(' ')?;
    Some(Stamp {
        secs: secs.parse().ok()?,
        age: age.to_owned(),
    })
}

/// The entry names of a tree inside a commit; an absent tree is empty.
fn tree_names(ws: &Path, commit: &str, path: &str, git: &dyn GitRunner) -> BTreeSet<String> {
    let spec = format!("{commit}:{path}");
    git.run_capture(&workspace::repo_git(ws), &["ls-tree", "--name-only", &spec])
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The install pool's skill directories; a missing pool is empty (§3.3).
fn pool_names(pool: &Path) -> BTreeSet<String> {
    match std::fs::read_dir(pool) {
        Ok(rd) => rd
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n != ARCHIVED)
            .collect(),
        Err(_) => BTreeSet::new(),
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

#[cfg(test)]
mod tests;
