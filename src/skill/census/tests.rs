//! The census over real workspaces (`docs/DESIGN_LEARNING_LOOP.md` §5).
//!
//! Every date here is git's, so the fixtures assert *presence* and
//! *absence* of a date rather than its text: what the verb promises is
//! that a use is dated by the commit that made it and a patch by the
//! commit that landed it, never that an age reads a particular way.

use super::{ABSENT, Owner, State, census, render};
use crate::harness_root::Roots;
use crate::template::{GitRunner, RealGit, scaffold};
use crate::workspace::{self, DEFAULT_CONFIG_NAME, fixture};
use std::path::Path;
use tempfile::TempDir;

/// A minimal valid SKILL.md — the authoring pass parses every one it
/// snapshots, so a fixture body must be real frontmatter.
fn skill_md(name: &str) -> String {
    format!("---\nname: {name}\ndescription: fixture skill\n---\n\nbody\n")
}

/// The named lineage's tip commit.
fn tip(ws: &Path) -> String {
    RealGit::new()
        .run_capture(
            &workspace::repo_git(ws),
            &[
                "rev-parse",
                &format!("config/{DEFAULT_CONFIG_NAME}^{{commit}}"),
            ],
        )
        .unwrap()
        .trim()
        .to_owned()
}

/// Land the `load_skill` copy of `name` on `agent`'s branch — the
/// election, in the only form the census can read: a commit that added
/// `skills/<name>/` on an `agents/*` ref.
fn elect(wt: &Path, name: &str) {
    let g = RealGit::new();
    let dir = wt.join("skills").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("SKILL.md"), skill_md(name)).unwrap();
    g.run(wt, &["add", "skills"]).unwrap();
    g.run(wt, &["commit", "-m", "load_skill"]).unwrap();
}

/// Find one row by name, or say which names there were.
fn row<'a>(rows: &'a [super::Row], name: &str) -> &'a super::Row {
    let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
    rows.iter()
        .find(|r| r.name == name)
        .unwrap_or_else(|| panic!("no row for {name} among {names:?}"))
}

/// The ball's proving fixture: a loaded pool skill, an unloaded
/// workspace skill and an archived one — three rows, three states, with
/// the loading commit dating the use and the config commit the patch.
#[test]
fn three_skills_three_states() {
    let (holder, ws) = fixture::workspace();
    fixture::amend_config(
        &ws,
        &[
            ("skills/note-taking/SKILL.md", &skill_md("note-taking")),
            ("skills/archived/legacy/SKILL.md", &skill_md("legacy")),
        ],
    );
    let wt = fixture::spawn_root(&ws, "20260101-a1");
    elect(&wt, "bash");
    let rows = census(&ws, &tip(&ws), &holder.path().join("data"), &RealGit::new());

    let loaded = row(&rows, "bash");
    assert_eq!(loaded.owner, Owner::Pool);
    assert_eq!(loaded.state, State::Active);
    assert!(loaded.last_use.is_some(), "the election dates the use");
    assert!(
        loaded.last_patch.is_none(),
        "the pool is the install's; no config commit patches it"
    );

    let unloaded = row(&rows, "note-taking");
    assert_eq!(unloaded.owner, Owner::Workspace);
    assert_eq!(unloaded.state, State::Unused);
    assert!(unloaded.last_use.is_none(), "nothing has elected it");
    assert!(
        unloaded.last_patch.is_some(),
        "the config commit that authored it dates the patch"
    );

    let archived = row(&rows, "legacy");
    assert_eq!(archived.owner, Owner::Workspace);
    assert_eq!(archived.state, State::Archived);
    assert!(archived.last_patch.is_some(), "the move is a config commit");

    // The product carries what the derivation decided: three states,
    // named, in one table.
    let table = render(&rows);
    for want in [
        "bash",
        "note-taking",
        "legacy",
        "active",
        "unused",
        "archived",
    ] {
        assert!(table.contains(want), "{want} missing from\n{table}");
    }
}

/// A pool skill a tool claims composes as that tool's description on
/// every model call, so it is never idle — §5's exemption, and the one
/// row that is `active` with no use at all.
#[test]
fn a_tool_claimed_pool_skill_is_active_with_no_use() {
    let (holder, ws) = fixture::workspace();
    let rows = census(&ws, &tip(&ws), &holder.path().join("data"), &RealGit::new());
    let claimed = row(&rows, "read_file");
    assert!(claimed.last_use.is_none(), "no branch has elected it");
    assert_eq!(claimed.state, State::Active);
}

/// A row with a use `secs` seconds into the epoch, or none.
fn dated(name: &str, secs: Option<i64>) -> super::Row {
    super::Row {
        name: name.to_owned(),
        owner: Owner::Pool,
        state: State::Active,
        last_use: secs.map(|secs| super::Stamp {
            secs,
            age: "an age".to_owned(),
        }),
        last_patch: None,
    }
}

/// Oldest-used first, a never-used skill oldest of all, ties broken by
/// name. Asserted over constructed stamps rather than over two real
/// commits a wall clock might date identically — where a stamp comes
/// from is what the git-backed fixtures above prove.
#[test]
fn rows_are_oldest_used_first() {
    let mut rows = vec![
        dated("newest", Some(300)),
        dated("b-never", None),
        dated("oldest", Some(100)),
        dated("a-never", None),
        dated("tied", Some(300)),
    ];
    super::order(&mut rows);
    let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["a-never", "b-never", "oldest", "newest", "tied"]);
}

/// A workspace with no skills in either home prints the headers and
/// nothing else — the general path with empty inputs, not an arm.
#[test]
fn a_workspace_with_no_skills_renders_headers_only() {
    let holder = TempDir::new().unwrap();
    let ws = holder.path().join("ws");
    let roots = Roots {
        config: holder.path().join("no-conf"),
        data: holder.path().join("no-pool"),
    };
    scaffold(&ws, &roots, &RealGit::new()).unwrap();
    let rows = census(&ws, &tip(&ws), &roots.data, &RealGit::new());
    assert!(rows.is_empty(), "{rows:?}");
    assert_eq!(render(&rows), "SKILL  OWNER  STATE  LAST USE  LAST PATCH");
}

/// The table is column-aligned to its widest cell and carries git's own
/// age text, with [`ABSENT`] where there is no date.
#[test]
fn the_table_aligns_its_columns() {
    let (holder, ws) = fixture::workspace();
    fixture::amend_config(
        &ws,
        &[("skills/note-taking/SKILL.md", &skill_md("note-taking"))],
    );
    let rows = census(&ws, &tip(&ws), &holder.path().join("data"), &RealGit::new());
    let table = render(&rows);
    let mut lines = table.lines();
    assert_eq!(
        lines.next().unwrap(),
        "SKILL           OWNER      STATE   LAST USE  LAST PATCH"
    );
    let unused = lines
        .find(|l| l.starts_with("note-taking"))
        .expect("the workspace skill has a row");
    assert!(
        unused.starts_with(&format!(
            "note-taking     workspace  unused  {ABSENT}       "
        )),
        "{unused}"
    );
}
