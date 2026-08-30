//! `litany prime` end-to-end over the real binary (ARCH §2.2). Proves the
//! exact invocation yog drives — `LITANY_HOME=<dir> litany prime` — founds
//! a fresh nested home, is idempotent (a second run changes nothing), and
//! never clobbers a hand-edited `models.yaml`. Product-less per the stdout
//! one-product convention (ARCH §3.4): stdout stays empty, while the
//! founding report — both roots and this run's seed-if-absent split —
//! rides stderr as a confirmation (bl-7e9e).

use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

fn prime(home: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_litany"))
        .arg("prime")
        .env("LITANY_HOME", home)
        .output()
        .expect("spawn litany prime")
}

/// Assert success, an empty stdout, and answer the stderr report so the
/// caller can read the seed-if-absent split out of it.
fn assert_ok_quiet(out: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(out.status.success(), "litany prime failed: {stderr}");
    assert!(
        out.stdout.is_empty(),
        "prime is product-less; stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    stderr
}

#[test]
fn prime_founds_a_fresh_nested_home_idempotently() {
    let home = TempDir::new().unwrap();
    let h = home.path();

    // First run founds the substrate — and says so, naming both roots
    // (collapsed to one by LITANY_HOME) and what it wrote (bl-7e9e).
    let first = assert_ok_quiet(&prime(h));
    assert!(
        first.contains(&format!("litany prime: config root {} —", h.display()))
            && first.contains(&format!("litany prime: data root {} —", h.display())),
        "the report names both roots; got {first:?}"
    );
    assert!(
        first.contains("0 already present and left alone"),
        "a fresh home keeps nothing; got {first:?}"
    );
    let seeded = seeded_count(&first);
    assert!(seeded > 0, "a fresh home seeds files; got {first:?}");
    assert!(h.join("workflows").is_dir());
    assert!(h.join("workspaces").is_dir());
    // The seeded models.yaml is mechanism only (bl-35e2): present, but
    // naming no model.
    let models = h.join("models.yaml");
    let body = fs::read_to_string(&models).unwrap();
    assert!(
        body.contains("adapter:"),
        "the adapter override is documented"
    );
    assert!(!body.contains("claude-"), "no model id ships (bl-35e2)");
    for name in [
        "bash",
        "cd",
        "dispatch",
        "load_skill",
        "message",
        "read_file",
    ] {
        assert!(h.join("tools").join(format!("{name}.json")).is_file());
        assert!(h.join("skills").join(name).join("SKILL.md").is_file());
    }

    // Idempotency: a hand-edited models.yaml survives a second run, and
    // nothing else the second run touched changed.
    fs::write(&models, "adapter: /opt/bz\n").unwrap();
    let second = assert_ok_quiet(&prime(h));
    assert_eq!(fs::read_to_string(&models).unwrap(), "adapter: /opt/bz\n");
    assert!(h.join("skills/bash/SKILL.md").is_file());
    // The re-run report is the first run's counts, swapped: nothing
    // seeded, everything kept — "already founded", stated in numbers.
    assert!(
        second.contains(&format!(
            "0 files seeded, {seeded} already present and left alone"
        )),
        "got {second:?}"
    );
}

/// The `N files seeded` count out of a report line.
fn seeded_count(report: &str) -> usize {
    report
        .split_once("founded: ")
        .and_then(|(_, rest)| rest.split_once(" files seeded"))
        .map(|(n, _)| n.parse().expect("a count"))
        .expect("the report states a seeded count")
}

#[test]
fn prime_reports_a_seeding_failure_loudly() {
    // A `LITANY_HOME` whose parent is a regular file cannot be created
    // (`ENOTDIR`), so seeding fails: the binding prints the uniform
    // `litany prime: …` stderr shape and exits non-zero (§3.4).
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("not-a-dir");
    fs::write(&file, b"x").unwrap();
    let out = prime(&file.join("home"));
    assert!(!out.status.success(), "seeding under a file must fail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("litany prime:"), "got {stderr:?}");
}
