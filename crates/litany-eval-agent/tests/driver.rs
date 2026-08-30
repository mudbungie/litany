//! Coverage for the harness driver (the far side of the ARCH §9.3
//! agent seam), exercised against a shell stub standing in for the
//! `litany` binary — the same no-live-traffic discipline as the
//! runner's own tests. The stub honours the `litany` surface the
//! driver touches (`new` / `config` / `prompt`), including `config`'s
//! `exec $EDITOR "$1"` hand-off, so the experiment-application path is
//! exercised for real. The live-wire proof is `make eval` itself.

use litany_eval_agent::{Contract, drive, grounded, machine_config_root};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

/// Serializes script-write-then-spawn pairs across tests (the ETXTBSY
/// trap: a concurrent spawn elsewhere in the binary briefly holds the
/// just-written script's fd open — one lock for the whole binary).
static SPAWN_LOCK: Mutex<()> = Mutex::new(());

fn spawn_lock() -> MutexGuard<'static, ()> {
    SPAWN_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Write an executable `sh` stub and return its path.
fn stub(dir: &Path, body: &str) -> PathBuf {
    let path = dir.join("litany");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    let mut perm = std::fs::metadata(&path).unwrap().permissions();
    perm.set_mode(0o755);
    std::fs::set_permissions(&path, perm).unwrap();
    path
}

/// A stub honouring the full surface the driver drives: `new` prints a
/// workspace it creates, `config` materializes a checkout with a
/// default `workflow.yaml` and runs `$EDITOR` on it exactly as
/// `litany config` does, `prompt` logs its message and prints an agent
/// id. Every invocation's env lands in `<log>/env.<verb>`.
fn full_stub(dir: &Path) -> PathBuf {
    let log = dir.display();
    stub(
        dir,
        &format!(
            r#"printf '%s\n' "$LITANY_HOME" "$LITANY_EXPERIMENT" "$GIT_DIR" > "{log}/env.$1"
case "$1" in
  new)
    mkdir -p "{log}/ws/.config-author"
    printf 'events: {{}}\n' > "{log}/ws/.config-author/workflow.yaml"
    echo "{log}/ws" ;;
  config)
    sh -c "exec $EDITOR \"\$1\"" sh "$2/.config-author" ;;
  prompt)
    printf '%s' "$3" > "{log}/prompt.msg"
    echo agent-a1b2 ;;
esac"#
        ),
    )
}

fn contract(dir: &Path, report: bool) -> Contract {
    let home = dir.join("home");
    let work = dir.join("work");
    std::fs::create_dir_all(&work).unwrap();
    let experiment = dir.join("workflow.yaml");
    std::fs::write(&experiment, "events: {x: [y]}\n").unwrap();
    Contract {
        prompt: "do the thing".into(),
        litany_home: home,
        experiment,
        report: report.then(|| dir.join("report")),
        workdir: work,
    }
}

#[test]
fn drives_new_config_prompt_and_reports() {
    let _g = spawn_lock();
    let d = tempfile::tempdir().unwrap();
    let litany = full_stub(d.path());
    let c = contract(d.path(), true);

    drive(litany.as_os_str(), None, &c).unwrap();

    // The experiment landed as the checkout's workflow.yaml (END 2 of
    // the seam: applied, not just handed off).
    assert_eq!(
        std::fs::read_to_string(d.path().join("ws/.config-author/workflow.yaml")).unwrap(),
        "events: {x: [y]}\n",
    );
    // The prompt was grounded in the shared workdir.
    let msg = std::fs::read_to_string(d.path().join("prompt.msg")).unwrap();
    assert_eq!(msg, grounded("do the thing", &c.workdir));
    assert!(msg.contains(c.workdir.to_str().unwrap()));
    assert!(msg.ends_with("do the thing"));
    // The report file carries workspace then agent id.
    assert_eq!(
        std::fs::read_to_string(d.path().join("report")).unwrap(),
        format!("{}/ws\nagent-a1b2\n", d.path().display()),
    );
    // Every litany invocation saw the run env, and no GIT_DIR leak.
    for verb in ["new", "config", "prompt"] {
        let env = std::fs::read_to_string(d.path().join(format!("env.{verb}"))).unwrap();
        let lines: Vec<&str> = env.lines().collect();
        assert_eq!(lines[0], c.litany_home.to_str().unwrap(), "{verb}");
        assert_eq!(lines[1], c.experiment.to_str().unwrap(), "{verb}");
        assert_eq!(lines[2], "", "GIT_DIR must be scrubbed for {verb}");
    }
}

#[test]
fn no_report_requested_writes_none() {
    let _g = spawn_lock();
    let d = tempfile::tempdir().unwrap();
    let litany = full_stub(d.path());
    let c = contract(d.path(), false);
    // A poisoned GIT_DIR in the driver's own env must not reach litany.
    unsafe { std::env::set_var("GIT_DIR", "/nowhere/.git") };
    drive(litany.as_os_str(), None, &c).unwrap();
    unsafe { std::env::remove_var("GIT_DIR") };
    assert!(!d.path().join("report").exists());
}

#[test]
fn seeds_run_home_from_machine_config_if_absent() {
    let _g = spawn_lock();
    let d = tempfile::tempdir().unwrap();
    let litany = full_stub(d.path());
    let c = contract(d.path(), false);

    let machine = d.path().join("machine");
    std::fs::create_dir_all(machine.join("template/souls")).unwrap();
    std::fs::write(machine.join("models.yaml"), "models: {m: 1}\n").unwrap();
    std::fs::write(machine.join("template/providers.yaml"), "roles: {}\n").unwrap();
    std::fs::write(machine.join("template/souls/worker.md"), "w\n").unwrap();
    // Pre-existing run-home material wins (seed-if-absent).
    std::fs::create_dir_all(c.litany_home.join("template")).unwrap();
    std::fs::write(
        c.litany_home.join("template/providers.yaml"),
        "roles: {kept: 1}\n",
    )
    .unwrap();

    drive(litany.as_os_str(), Some(&machine), &c).unwrap();

    let home = &c.litany_home;
    assert_eq!(
        std::fs::read_to_string(home.join("models.yaml")).unwrap(),
        "models: {m: 1}\n"
    );
    assert_eq!(
        std::fs::read_to_string(home.join("template/providers.yaml")).unwrap(),
        "roles: {kept: 1}\n",
        "seed-if-absent: the run home's own file must survive"
    );
    assert_eq!(
        std::fs::read_to_string(home.join("template/souls/worker.md")).unwrap(),
        "w\n"
    );
}

#[test]
fn absent_machine_config_and_present_home_models_are_fine() {
    let _g = spawn_lock();
    let d = tempfile::tempdir().unwrap();
    let litany = full_stub(d.path());
    let c = contract(d.path(), false);
    // Machine root exists but has neither models.yaml nor template/.
    let machine = d.path().join("machine-empty");
    std::fs::create_dir_all(&machine).unwrap();
    // The run home already carries a models.yaml — it must survive.
    std::fs::create_dir_all(&c.litany_home).unwrap();
    std::fs::write(c.litany_home.join("models.yaml"), "models: {mine: 1}\n").unwrap();
    std::fs::write(machine.join("models.yaml"), "models: {machine: 1}\n").unwrap();

    drive(litany.as_os_str(), Some(&machine), &c).unwrap();

    assert_eq!(
        std::fs::read_to_string(c.litany_home.join("models.yaml")).unwrap(),
        "models: {mine: 1}\n"
    );
    // A machine root that does not exist at all is nothing to seed.
    let c2 = contract(d.path(), false);
    drive(litany.as_os_str(), Some(&d.path().join("no-such")), &c2).unwrap();
}

#[test]
fn a_failing_verb_surfaces_its_stderr() {
    let _g = spawn_lock();
    let d = tempfile::tempdir().unwrap();
    let litany = stub(d.path(), "echo 'new: it broke' >&2\nexit 3");
    let c = contract(d.path(), false);
    let err = drive(litany.as_os_str(), None, &c).unwrap_err();
    assert!(err.to_string().contains("litany new exited"), "{err}");
    assert!(err.to_string().contains("it broke"), "{err}");
}

#[test]
fn an_empty_product_is_refused() {
    let _g = spawn_lock();
    let d = tempfile::tempdir().unwrap();
    let litany = stub(d.path(), "exit 0");
    let c = contract(d.path(), false);
    let err = drive(litany.as_os_str(), None, &c).unwrap_err();
    assert!(
        err.to_string().contains("litany new printed no product"),
        "{err}"
    );
}

#[test]
fn a_missing_litany_names_the_program() {
    let _g = spawn_lock();
    let d = tempfile::tempdir().unwrap();
    let c = contract(d.path(), false);
    let missing = d.path().join("no-such-litany");
    let err = drive(missing.as_os_str(), None, &c).unwrap_err();
    assert!(err.to_string().contains("no-such-litany"), "{err}");
}

#[test]
fn contract_assembly_names_each_missing_piece() {
    let work = PathBuf::from("/w");
    let full = Contract::assemble(
        Some("p".into()),
        Some("/h".into()),
        Some("/e".into()),
        None,
        work.clone(),
    )
    .unwrap();
    assert_eq!(full.prompt, "p");
    assert!(full.report.is_none());

    let e = Contract::assemble(
        None,
        Some("/h".into()),
        Some("/e".into()),
        None,
        work.clone(),
    );
    assert_eq!(e.unwrap_err(), "no prompt on argv[1]");
    let e = Contract::assemble(
        Some("p".into()),
        None,
        Some("/e".into()),
        None,
        work.clone(),
    );
    assert_eq!(e.unwrap_err(), "LITANY_HOME is not set");
    let e = Contract::assemble(Some("p".into()), Some("/h".into()), None, None, work);
    assert_eq!(e.unwrap_err(), "LITANY_EXPERIMENT is not set");
}

#[test]
fn machine_config_root_prefers_xdg_then_home() {
    assert_eq!(
        machine_config_root(Some("/xdg".as_ref()), Some("/me".as_ref())),
        Some(PathBuf::from("/xdg/litany"))
    );
    assert_eq!(
        machine_config_root(Some("".as_ref()), Some("/me".as_ref())),
        Some(PathBuf::from("/me/.config/litany")),
        "an empty XDG_CONFIG_HOME is unset, per the basedir spec"
    );
    assert_eq!(
        machine_config_root(None, Some("/me".as_ref())),
        Some(PathBuf::from("/me/.config/litany"))
    );
    assert_eq!(machine_config_root(None, Some("".as_ref())), None);
    assert_eq!(machine_config_root(None, None), None);
}

#[test]
fn version_answer_serves_the_probe_and_nothing_else() {
    use litany_eval_agent::version_answer;
    let line = version_answer(Some("--version")).unwrap();
    assert_eq!(
        line,
        format!("litany-eval-agent {}", env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(version_answer(Some("a real prompt")), None);
    assert_eq!(version_answer(None), None);
}
