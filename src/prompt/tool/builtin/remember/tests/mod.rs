//! Unit tests for [`super::run_with`] (`docs/DESIGN_CONTEXT_ECONOMY.md`
//! §3): the door an agent records a durable fact through, and every way
//! it declines. The subject is driven over a real workspace fixture,
//! because the whole claim is what lands in git.

use super::*;
use crate::template::RealGit;
use crate::workspace::{config_ref, fixture, proposal::proposal_ref, repo_git};
use std::collections::HashMap;
use std::ffi::OsString;
use std::io::Cursor;
use tempfile::TempDir;

/// HashMap-backed stub [`EnvLookup`] — `None` for anything not seeded.
pub(super) struct StubEnv(pub(super) HashMap<&'static str, OsString>);
impl EnvLookup for StubEnv {
    fn get(&self, key: &str) -> Option<OsString> {
        self.0.get(key).cloned()
    }
}

/// A workspace with one root agent, and the §3.3 contract its tools are
/// spawned with. `home` is the harness root the fixture primed.
pub(super) fn caller(home: &Path, ws: &Path, agent: &str) -> StubEnv {
    let mut m = HashMap::new();
    m.insert(ENV_CONV_REPO, ws.as_os_str().to_owned());
    m.insert(ENV_CONV_BRANCH, OsString::from(agent));
    m.insert(
        super::super::harness::ENV_LITANY_HOME,
        home.as_os_str().to_owned(),
    );
    StubEnv(m)
}

pub(super) const AGENT: &str = "20260101-a1";

/// The fixture: a primed harness root, a workspace under it, and a root
/// agent forked off `config/default`.
pub(super) fn fixture() -> (TempDir, PathBuf, PathBuf) {
    let holder = TempDir::new().unwrap();
    let home = holder.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let roots = crate::harness_root::Roots {
        config: home.clone(),
        data: home.clone(),
    };
    let ws = fixture::workspace_under(&roots);
    fixture::spawn_root(&ws, AGENT);
    (holder, home, ws)
}

pub(super) fn remember(home: &Path, ws: &Path, fact: &str) -> Result<String, Error> {
    let env = caller(home, ws, AGENT);
    let mut out: Vec<u8> = Vec::new();
    let input = serde_json::to_vec(&serde_json::json!({ "fact": fact })).unwrap();
    run_with(&mut Cursor::new(input), &mut out, &env, &RealGit::new())?;
    Ok(String::from_utf8(out).unwrap())
}

/// Read `facts.md` off a ref, or `None` when the tree carries none.
pub(super) fn facts(ws: &Path, refname: &str) -> Option<String> {
    let spec = format!("{refname}:{}", crate::facts::FILE);
    RealGit::new()
        .run_capture(&repo_git(ws), &["show", &spec])
        .ok()
}

/// The whole claim: a fact reaches `facts.md` on a proposal branch, the
/// lineage is untouched, and the model is handed the command that
/// settles it.
#[test]
fn a_fact_is_staged_as_a_proposal_and_no_lineage_moves() {
    let (_h, home, ws) = fixture();
    let git = RealGit::new();
    let before = git
        .run_capture(&repo_git(&ws), &["rev-parse", &config_ref("default")])
        .unwrap();

    let out = remember(&home, &ws, "raw observations are kept for 45 days").unwrap();
    assert!(out.contains("\"status\":\"proposed\""), "{out}");
    assert!(out.contains(AGENT), "{out}");
    assert!(out.contains("--accept"), "{out}");

    assert_eq!(
        facts(&ws, &proposal_ref(AGENT)).as_deref(),
        Some("raw observations are kept for 45 days"),
        "the fact is on the proposal branch"
    );
    assert_eq!(facts(&ws, &config_ref("default")), None, "and nowhere else");
    assert_eq!(
        before,
        git.run_capture(&repo_git(&ws), &["rev-parse", &config_ref("default")])
            .unwrap(),
        "the lineage did not move"
    );
    let subject = git
        .run_capture(
            &repo_git(&ws),
            &["log", "-1", "--format=%s", &proposal_ref(AGENT)],
        )
        .unwrap();
    assert_eq!(subject, "facts: raw observations are kept for 45 days");
}

/// A second fact amends the standing proposal rather than stacking on
/// it: one branch, one commit, still parented on the lineage head — so
/// `litany proposal` still reads it fresh.
#[test]
fn a_second_fact_amends_the_standing_proposal_and_it_stays_fresh() {
    let (_h, home, ws) = fixture();
    let git = RealGit::new();
    remember(&home, &ws, "raw observations are kept for 45 days").unwrap();
    let out = remember(&home, &ws, "the ingest window closes at 02:00 UTC").unwrap();
    assert!(out.contains("\"status\":\"proposed\""), "{out}");

    assert_eq!(
        facts(&ws, &proposal_ref(AGENT)).as_deref(),
        Some("raw observations are kept for 45 days\n\nthe ingest window closes at 02:00 UTC"),
    );
    let count = git
        .run_capture(
            &repo_git(&ws),
            &[
                "rev-list",
                "--count",
                &format!("{}..{}", config_ref("default"), proposal_ref(AGENT)),
            ],
        )
        .unwrap();
    assert_eq!(count, "1", "one commit ahead of the lineage, not two");
    let rows = crate::workspace::proposal::list(&ws, &git).unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].fresh, "the amended proposal is still fresh");
}

/// Re-proposing what is already recorded changes nothing, costs no
/// commit, and says so — the authoring routine's declined pass, which
/// also takes back the branch it had created.
#[test]
fn a_fact_already_recorded_proposes_nothing() {
    let (_h, home, ws) = fixture();
    let out = remember(&home, &ws, "the same fact").unwrap();
    assert!(out.contains("\"status\":\"proposed\""), "{out}");
    let again = remember(&home, &ws, "  the same fact\n").unwrap();
    assert!(again.contains("\"status\":\"already_recorded\""), "{again}");
    assert_eq!(
        facts(&ws, &proposal_ref(AGENT)).as_deref(),
        Some("the same fact")
    );
}

/// The very first `remember` whose fact is already in the lineage stages
/// no branch at all: the declined pass deletes the ref it created.
#[test]
fn a_duplicate_of_a_recorded_fact_leaves_no_proposal_branch() {
    let (_h, home, ws) = fixture();
    // Authored through the routine over the *primed* root, so this pass
    // and the one under test see the same `descriptions/**` and the only
    // difference the tool can make is the fact itself.
    crate::template::authoring::author(
        &ws,
        &home,
        "default",
        crate::template::authoring::Origin::Advance,
        |dir| std::fs::write(dir.join(crate::facts::FILE), "already known\n"),
        &RealGit::new(),
    )
    .unwrap();
    let out = remember(&home, &ws, "already known").unwrap();
    assert!(out.contains("\"status\":\"already_recorded\""), "{out}");
    assert!(
        facts(&ws, &proposal_ref(AGENT)).is_none(),
        "no proposal branch was left behind"
    );
}

/// The cap is the artifact's and it is a refusal: an over-cap fact is
/// declined in the authoring routine's own voice, and nothing is staged.
#[test]
fn an_over_cap_fact_is_refused_and_stages_nothing() {
    let (_h, home, ws) = fixture();
    let huge = "x".repeat(crate::facts::MAX_BYTES as usize + 1);
    let err = remember(&home, &ws, &huge).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("facts.md"), "{msg}");
    assert!(
        facts(&ws, &proposal_ref(AGENT)).is_none(),
        "a refused pass stages nothing"
    );
    assert!(
        !ws.join(".config-author").exists(),
        "and heals its checkout"
    );
}

mod declines;
