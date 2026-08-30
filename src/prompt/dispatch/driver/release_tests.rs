//! Deterministic coverage of the §2.11 release rule (bl-9c8f). The
//! lost-wakeup window is [holder's last inbox read, holder's release];
//! the seen-set *names* that read, so a test holds the window open by
//! construction — deposit after the set was taken, then release — no
//! sleeps, no widened drains, no e2e flake-hunting.

use super::super::drain::SeenDeposit;
use super::{release_then_reprobe, reprobe_after_release};
use crate::prompt::SystemClock;
use crate::prompt::inbox::{
    Epitaph, Launcher, ProbeOutcome, deposit, deposit_result, inbox_dir, probe_and_launch,
    try_acquire,
};
use std::cell::RefCell;
use std::io;
use std::path::Path;
use tempfile::TempDir;

/// The [`SeenDeposit`] identity of a real deposited file: its name plus
/// its on-disk mtime — what a drain that enumerated and held it records.
fn seen_of(path: &Path) -> SeenDeposit {
    SeenDeposit::new(
        path.file_name().unwrap().to_string_lossy().into_owned(),
        std::fs::metadata(path).unwrap().modified().unwrap(),
    )
}

/// A root-shaped agent id (two hyphen-free tokens, §2.3).
const AGENT: &str = "20260101-a1";

/// A no-op [`crate::template::GitRunner`] for deposits into a bare
/// `TempDir` workspace: the returned-mark `update-ref` has no repo.git
/// to land in here, and these tests read only the inbox files.
struct OkGit;
impl crate::template::GitRunner for OkGit {
    fn run(&self, _dest: &Path, _args: &[&str]) -> io::Result<()> {
        Ok(())
    }
    fn run_capture(&self, _dest: &Path, _args: &[&str]) -> io::Result<String> {
        Ok(String::new())
    }
}

/// Recording [`Launcher`]: what the release rule fired, and at whom.
#[derive(Default)]
struct RecLauncher {
    launches: RefCell<Vec<String>>,
}
impl Launcher for RecLauncher {
    fn launch(&self, _ws: &Path, agent: &str) -> io::Result<()> {
        self.launches.borrow_mut().push(agent.to_string());
        Ok(())
    }
}

/// A launcher that cannot spawn — the fire-and-forget swallow arm.
struct FailLauncher;
impl Launcher for FailLauncher {
    fn launch(&self, _ws: &Path, _agent: &str) -> io::Result<()> {
        Err(io::Error::other("spawn refused"))
    }
}

#[test]
fn a_deposit_racing_the_holders_last_read_is_launched_at_release() {
    // The proven strand (bl-9c8f), replayed deterministically: holder H
    // acquires; H's last inbox read sees nothing (seen = []); writer W
    // deposits; W's probe reads Busy and defers to H — the deferral that
    // used to strand the deposit forever; H releases through the rule
    // and completes W's launch.
    let ws = TempDir::new().unwrap();
    let holder = try_acquire(&inbox_dir(ws.path(), AGENT))
        .unwrap()
        .expect("free");
    let seen: Vec<SeenDeposit> = Vec::new();
    deposit(ws.path(), AGENT, "user", "racing mail", &SystemClock).unwrap();
    let rec = RecLauncher::default();
    assert_eq!(
        probe_and_launch(ws.path(), AGENT, &rec).unwrap(),
        ProbeOutcome::Busy,
        "the writer's probe must defer to the live holder"
    );
    assert!(rec.launches.borrow().is_empty());
    release_then_reprobe(holder, ws.path(), AGENT, &seen, &rec);
    // The launch fired — which also proves the lease really released,
    // since the rule's own probe would read Busy against a held lock.
    assert_eq!(*rec.launches.borrow(), vec![AGENT.to_string()]);
    // Launching is not delivering: the mail still awaits its driver.
    assert_eq!(
        std::fs::read_dir(inbox_dir(ws.path(), AGENT))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn mail_the_holders_last_read_saw_never_relaunches() {
    // Termination: a deposit the holder enumerated and deliberately left
    // (the §6 gate-hold shape) is in the seen-set, so the release fires
    // nothing — hold → no-op driver → hold cannot relaunch-loop.
    let ws = TempDir::new().unwrap();
    let holder = try_acquire(&inbox_dir(ws.path(), AGENT))
        .unwrap()
        .expect("free");
    let held = deposit(ws.path(), AGENT, "user", "held mail", &SystemClock).unwrap();
    let seen = vec![seen_of(&held)];
    let rec = RecLauncher::default();
    release_then_reprobe(holder, ws.path(), AGENT, &seen, &rec);
    assert!(rec.launches.borrow().is_empty());
}

#[test]
fn a_reused_name_is_a_new_deposit_and_fires_the_release() {
    // Deposit names are max-present-plus-one over the current listing
    // (§2.11 *Deposit*), so mail the holder delivered or interpreted
    // frees its name for a racing writer to mint again. Identity is the
    // (name, mtime) file pair, never the name: a seen name with a fresh
    // mtime is a new deposit, owed its launch like any other.
    let ws = TempDir::new().unwrap();
    let holder = try_acquire(&inbox_dir(ws.path(), AGENT))
        .unwrap()
        .expect("free");
    let racing = deposit(ws.path(), AGENT, "user", "racing mail", &SystemClock).unwrap();
    // A seen-set naming the same file name at a different instant — the
    // consumed predecessor whose name the racing deposit reused.
    let seen = vec![SeenDeposit::new(
        racing.file_name().unwrap().to_string_lossy().into_owned(),
        std::time::SystemTime::UNIX_EPOCH,
    )];
    let rec = RecLauncher::default();
    release_then_reprobe(holder, ws.path(), AGENT, &seen, &rec);
    assert_eq!(*rec.launches.borrow(), vec![AGENT.to_string()]);
}

#[test]
fn a_rival_holding_the_freed_lease_defers_the_reprobe() {
    // No double-drive: the re-read runs strictly after the release, so a
    // rival that won the freed lease first turns the releaser's probe
    // into the ordinary Busy deferral (Writer/driver totality) — and the
    // rival's own release re-reads when it comes.
    let ws = TempDir::new().unwrap();
    let _rival = try_acquire(&inbox_dir(ws.path(), AGENT))
        .unwrap()
        .expect("free");
    deposit(ws.path(), AGENT, "user", "new mail", &SystemClock).unwrap();
    let rec = RecLauncher::default();
    reprobe_after_release(ws.path(), AGENT, &[], &rec);
    assert!(rec.launches.borrow().is_empty());
}

#[test]
fn a_racing_result_is_completed_per_its_own_epitaph_warrant() {
    // The dual completes the launch the racing writer was *owed*, no
    // more — pin 2's one epitaph decision, replayed at the raced
    // boundary so a deposit's fate never turns on the release
    // millisecond: a final-response return would have probe-and-launched
    // had it landed after the release (the child's own revive_parent),
    // so the releaser completes it; a stopped or budget-exhausted return
    // never launches, so the release parks it exactly as the unraced
    // path does (§2.11 "stays undelivered ... the next explicit touch").
    let child = "20260101-a1-20260102-b2";
    for (epitaph, launches) in [
        (Epitaph::FinalResponse, true),
        (Epitaph::Died, true),
        (Epitaph::Stopped, false),
        (Epitaph::BudgetExhausted, false),
    ] {
        let ws = TempDir::new().unwrap();
        deposit_result(
            ws.path(),
            AGENT,
            child,
            epitaph,
            "abc123",
            None,
            &SystemClock,
            &OkGit,
        )
        .unwrap();
        let rec = RecLauncher::default();
        reprobe_after_release(ws.path(), AGENT, &[], &rec);
        assert_eq!(
            !rec.launches.borrow().is_empty(),
            launches,
            "epitaph {:?} must {}launch",
            epitaph,
            if launches { "" } else { "not " }
        );
    }
}

#[test]
fn an_illegible_deposit_body_launches_rather_than_classifies() {
    // No launcher decides warrant (§2.11): a deposit whose body cannot
    // be read gets the launch, and the launched driver sorts it out
    // under the lock. A directory wearing a deposit's name is the
    // deterministic unreadable body.
    let ws = TempDir::new().unwrap();
    std::fs::create_dir_all(inbox_dir(ws.path(), AGENT).join("user-001.md")).unwrap();
    let rec = RecLauncher::default();
    reprobe_after_release(ws.path(), AGENT, &[], &rec);
    assert_eq!(*rec.launches.borrow(), vec![AGENT.to_string()]);
}

#[test]
fn an_unreadable_inbox_reread_is_swallowed() {
    // Accepted crash class (§2.11): the lease is already gone, so a
    // failed re-read may only log — the stranding is late, never lost,
    // and the next touch (`litany scan`, a reprompt) heals it.
    let ws = TempDir::new().unwrap();
    std::fs::create_dir_all(ws.path().join("inbox")).unwrap();
    std::fs::write(inbox_dir(ws.path(), AGENT), b"not a dir").unwrap();
    let rec = RecLauncher::default();
    reprobe_after_release(ws.path(), AGENT, &[], &rec);
    assert!(rec.launches.borrow().is_empty());
}

#[test]
fn a_failed_post_release_launch_is_swallowed() {
    // Fire-and-forget (§2.11): unseen mail over a free lease warrants a
    // launch, and a spawn refusal is logged and swallowed, never raised.
    let ws = TempDir::new().unwrap();
    deposit(ws.path(), AGENT, "user", "mail", &SystemClock).unwrap();
    reprobe_after_release(ws.path(), AGENT, &[], &FailLauncher);
}
