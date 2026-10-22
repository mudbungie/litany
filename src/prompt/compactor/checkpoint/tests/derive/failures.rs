//! [`state`]'s per-op git failures, each tagged with the op that failed —
//! split from [`super`] along that seam to hold the per-file line cap.

use super::*;

// ---- per-op git failures, via a stub -----------------------------------
//
// Real git can only fail wholesale (the test above); each later derivation
// step's `op` tag needs a git that fails at exactly that step. The stub
// answers every capture except the one whose args contain `fail_on`.

struct FailOn(&'static str);

impl GitRunner for FailOn {
    fn run(&self, _dest: &Path, _args: &[&str]) -> std::io::Result<()> {
        unreachable!("checkpoint derivation only captures")
    }
    fn run_capture(&self, _dest: &Path, args: &[&str]) -> std::io::Result<String> {
        if args.iter().any(|a| a.contains(self.0)) {
            return Err(std::io::Error::other("stub git failure"));
        }
        // Benign answers: no checkpoint landed (empty grep), one commit,
        // a root sha, epoch second 100.
        Ok(match args {
            a if a.contains(&"--grep") => String::new(),
            a if a.contains(&"--count") => "1".to_string(),
            a if a.contains(&"--max-parents=0") => "r00t".to_string(),
            _ => "100".to_string(),
        })
    }
}

fn op_of(err: Error) -> &'static str {
    match err {
        Error::Git { op, .. } => op,
        other => panic!("expected Error::Git, got {other:?}"),
    }
}

#[test]
fn state_tags_a_commit_count_failure_with_its_op() {
    let dir = TempDir::new().unwrap();
    let err = state_of(dir.path(), "p1", 0, false, &FailOn("--count")).unwrap_err();
    assert_eq!(op_of(err), "checkpoint rev-list count");
}

#[test]
fn state_tags_a_commit_time_failure_with_its_op() {
    let dir = TempDir::new().unwrap();
    let err = state_of(dir.path(), "p1", 0, false, &FailOn("%ct")).unwrap_err();
    assert_eq!(op_of(err), "checkpoint commit time");
}

#[test]
fn state_tags_a_root_lookup_failure_with_its_op() {
    let dir = TempDir::new().unwrap();
    let err = state_of(dir.path(), "p1", 0, false, &FailOn("--max-parents=0")).unwrap_err();
    assert_eq!(op_of(err), "checkpoint root rev-list");
}

#[test]
fn state_tags_an_in_flight_enumeration_failure_with_its_op() {
    // bl-b9f0: the in-flight read is the registry query (§8's enumeration
    // seam, asked from the worktree), and a failure there is loud like
    // every other step of the derivation rather than a silent `false`.
    let dir = TempDir::new().unwrap();
    let err = state_of(dir.path(), "p1", 0, false, &FailOn("for-each-ref")).unwrap_err();
    assert_eq!(op_of(err), "checkpoint in-flight for-each-ref");
}
