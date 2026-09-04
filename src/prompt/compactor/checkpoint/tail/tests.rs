//! The token tail against a real repository (`docs/DESIGN_CONTEXT_ECONOMY.md`
//! §5.2): where the point lands, what it ignores, and the two empty-input
//! paths that compact nothing.

use super::*;
use crate::template::RealGit;
use tempfile::TempDir;

fn init(wt: &Path) {
    let g = RealGit::new();
    g.run(wt, &["init", "-b", "agents/p1"]).unwrap();
    g.run(wt, &["config", "user.email", "t@t"]).unwrap();
    g.run(wt, &["config", "core.hooksPath", "/dev/null"])
        .unwrap();
    g.run(wt, &["config", "user.name", "t"]).unwrap();
}

fn commit(wt: &Path, subject: &str, rel: &str, content: &str) -> String {
    let g = RealGit::new();
    let f = wt.join(rel);
    std::fs::create_dir_all(f.parent().unwrap()).unwrap();
    std::fs::write(&f, content).unwrap();
    g.run(wt, &["add", "-A"]).unwrap();
    g.run(wt, &["commit", "-m", subject]).unwrap();
    g.run_capture(wt, &["rev-parse", "HEAD"])
        .unwrap()
        .trim()
        .to_string()
}

/// One model entry landing in its own transcript commit, the shape the
/// transcript writer produces (§2.3).
fn entry(wt: &Path, seq: u32, prompt: u64) -> String {
    commit(
        wt,
        &format!("transcript {seq:03}: m [p1]"),
        &format!("messages/{seq:03}-m.json"),
        &format!(r#"{{"content":[],"usage":{{"input_tokens":{prompt}}}}}"#),
    )
}

/// A branch founded by its dispatch commit, then `prompts` model entries.
fn branch(wt: &Path, prompts: &[u64]) -> Vec<String> {
    init(wt);
    commit(wt, "step 001: dispatch [p1]", "goal.md", "g");
    prompts
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let seq = u32::try_from(i).unwrap() + 1;
            entry(wt, seq, *p)
        })
        .collect()
}

#[test]
fn the_point_is_the_oldest_commit_the_budget_still_reaches() {
    // Prompt sides 100, 300, 600, 1000; the tip is 1000 and the budget
    // is 500, so the retained tail may start at 600 (cost 400) but not
    // at 300 (cost 700). The point is therefore the 600 commit: the
    // stretch beneath it compacts, the stretch above it costs 400.
    let dir = TempDir::new().unwrap();
    let wt = dir.path();
    let shas = branch(wt, &[100, 300, 600, 1000]);
    let p = point(wt, "p1", 500, &RealGit::new()).unwrap();
    assert_eq!(p.as_deref(), Some(shas[2].as_str()));
    // One token less of budget and the tail may not reach that commit,
    // so the point moves up to the tip's own — an empty tail, which is
    // `keep_recent: 0` reached by the other unit.
    let p = point(wt, "p1", 399, &RealGit::new()).unwrap();
    assert_eq!(p.as_deref(), Some(shas[3].as_str()));
}

#[test]
fn a_branch_that_fits_the_budget_has_nothing_to_compact() {
    // The whole uncompacted stretch costs 900 to append and the budget
    // is 900: the point would sit at the checkpoint origin, the span is
    // empty, and the flush skips (§5.2) — the same answer a span inside
    // `keep_recent` gives.
    let dir = TempDir::new().unwrap();
    let wt = dir.path();
    branch(wt, &[100, 400, 1000]);
    assert_eq!(point(wt, "p1", 900, &RealGit::new()).unwrap(), None);
    // A branch with no model entry at all is the same answer, reached
    // before any git walk: nothing has stated a prompt count yet.
    let empty = TempDir::new().unwrap();
    init(empty.path());
    commit(empty.path(), "step 001: dispatch [p1]", "goal.md", "g");
    assert_eq!(point(empty.path(), "p1", 1, &RealGit::new()).unwrap(), None);
}

#[test]
fn the_walk_measures_from_the_checkpoint_origin_not_inherited_history() {
    // The clock's own lower bound (`reference::origin`): entries a
    // landing already swallowed are not this pass's to weigh, so a
    // compaction base cuts the walk even when older entries would have
    // exceeded the budget on their own.
    let dir = TempDir::new().unwrap();
    let wt = dir.path();
    branch(wt, &[100, 200]);
    commit(wt, "compaction base [p1-c1]", "summary/001.md", "s");
    let after = [entry(wt, 3, 900), entry(wt, 4, 1200)];
    // Measured from the base, the stretch costs 300 and fits a budget of
    // 400 — even though 1200 - 100 would not.
    assert_eq!(point(wt, "p1", 400, &RealGit::new()).unwrap(), None);
    // Tighten it below 300 and the point is the newer of the two.
    let p = point(wt, "p1", 200, &RealGit::new()).unwrap();
    assert_eq!(p.as_deref(), Some(after[1].as_str()));
}

#[test]
fn only_model_entries_are_candidates() {
    // The walk weighs model entries alone. A `tool` entry is filtered by
    // its reserved origin token — here carrying a fabricated `usage`, so
    // the filter is the origin and not merely the missing report — a
    // `.md` delivery by its extension, and a lawful bare-array model
    // entry by having no report to read (§2.3). None is a candidate, so
    // the point stays on a real step boundary.
    let dir = TempDir::new().unwrap();
    let wt = dir.path();
    init(wt);
    commit(wt, "step 001: dispatch [p1]", "goal.md", "g");
    entry(wt, 1, 100);
    let mid = entry(wt, 2, 600);
    commit(
        wt,
        "transcript 003: tool [p1]",
        "messages/003-tool.json",
        r#"{"content":[],"usage":{"input_tokens":650}}"#,
    );
    commit(wt, "transcript 004: u [p1]", "messages/004-u.md", "hi");
    commit(
        wt,
        "transcript 005: m [p1]",
        "messages/005-m.json",
        r#"[{"type":"text","text":"no usage"}]"#,
    );
    entry(wt, 6, 1000);
    // Budget 500: the 1000 tip costs nothing, the 600 entry costs 400
    // and fits, the 100 entry costs 900 and does not. Were the tool
    // entry weighed it would sit between them and take the point.
    let p = point(wt, "p1", 500, &RealGit::new()).unwrap();
    assert_eq!(p.as_deref(), Some(mid.as_str()));
}

#[test]
fn a_branch_with_no_founding_commit_walks_its_whole_history() {
    // `reference::origin` answers `None` for a branch carrying neither a
    // dispatch subject nor a landing one, and the range is then bare
    // `HEAD` — the general path with empty inputs, the same fallback the
    // clock takes, not a bootstrap special case. The walk is unchanged.
    let dir = TempDir::new().unwrap();
    let wt = dir.path();
    init(wt);
    commit(wt, "root", "a.txt", "1");
    entry(wt, 1, 100);
    let mid = entry(wt, 2, 600);
    entry(wt, 3, 1000);
    let p = point(wt, "p1", 500, &RealGit::new()).unwrap();
    assert_eq!(p.as_deref(), Some(mid.as_str()));
}

/// A `GitRunner` whose capture fails on the nth call, so each of the two
/// git steps can be failed independently.
struct FailAt(std::cell::Cell<usize>, usize);
impl GitRunner for FailAt {
    fn run(&self, _d: &Path, _a: &[&str]) -> std::io::Result<()> {
        unreachable!("the tail only captures")
    }
    fn run_capture(&self, d: &Path, args: &[&str]) -> std::io::Result<String> {
        let n = self.0.get();
        self.0.set(n + 1);
        if n == self.1 {
            return Err(std::io::Error::other("boom"));
        }
        RealGit::new().run_capture(d, args)
    }
}

#[test]
fn each_git_step_surfaces_under_its_own_op_tag() {
    let dir = TempDir::new().unwrap();
    let wt = dir.path();
    branch(wt, &[100, 1000]);
    // Call 0 is `reference::origin`'s grep, 1 the log, 2 the first blob.
    for (nth, op) in [(1, "token tail log"), (2, "token tail entry read")] {
        let err = point(wt, "p1", 1, &FailAt(std::cell::Cell::new(0), nth)).unwrap_err();
        assert!(
            matches!(&err, Error::Git { op: o, .. } if *o == op),
            "{err:?}"
        );
    }
}
