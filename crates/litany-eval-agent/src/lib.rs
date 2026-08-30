//! `litany-eval-agent` — the harness driver the evaluation runner
//! invokes per run (the far side of the ARCH §9.3 agent seam).
//!
//! The runner (`crates/agent-eval`) hands each run to an external
//! program under the contract in the repo README ("Run the suite"):
//! the task prompt on argv\[1\], `LITANY_HOME` / `LITANY_EXPERIMENT` /
//! `LITANY_EVAL_REPORT` in the env, and the working directory shared
//! with the task's `setup` and `check`. This crate is that program for
//! the litany harness itself. It is an integration, so it is an
//! external binary (`docs/PRINCIPLES.md`), **not** a `litany` verb —
//! and it drives the harness exclusively through the front door, by
//! exec'ing `litany` (resolved from `PATH`):
//!
//! 1. Seed the run's isolated `LITANY_HOME` from the machine's litany
//!    config root (see [`machine_config_root`]) — the wire is
//!    machine-local by design (ARCH §4.2, §9.2), so an isolated home
//!    reaches the operator's provider only if the machine's
//!    `models.yaml` and `template/` overrides travel into it. Both are
//!    the existing §4.2 / config-root-override front doors; the copy is
//!    seed-if-absent, exactly like `litany prime`.
//! 2. `litany new` — create the run's workspace.
//! 3. **Apply the experiment** (the quiet end of the seam):
//!    `litany config` with `$EDITOR` set to copy `LITANY_EXPERIMENT`
//!    over the checkout's `workflow.yaml`, so the experiment lands in
//!    the workspace's config commit — the only place the harness reads
//!    workflow policy from (ARCH §2.2). For `baseline` the copy changes
//!    nothing and the pass declines: an empty diff is already in force.
//! 4. `litany prompt` — one root agent, handed the task prompt grounded
//!    in the shared working directory (see [`grounded`]).
//! 5. Report the workspace path and agent id through
//!    `LITANY_EVAL_REPORT` (two lines), which is what `litany bundle`
//!    needs to archive a failing run (ARCH §9.2).
//!
//! The driver's exit code is ignored by contract — pass/fail is the
//! task `check` alone — so failures here only cost the run its agent
//! work; they are still reported on stderr for the operator.

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Answer the runner's version probe (bl-36fa): `--version` as
/// argv\[1\] yields the driver's one identifying line, anything else
/// yields `None` and the run contract proceeds. The probe is part of
/// the README "Run the suite" contract — the runner records the line
/// among an evaluation's reproducibility inputs.
pub fn version_answer(arg1: Option<&str>) -> Option<String> {
    (arg1 == Some("--version")).then(|| format!("litany-eval-agent {}", env!("CARGO_PKG_VERSION")))
}

/// What one invocation receives from the runner — the README "Run the
/// suite" driver contract, parsed and validated.
#[derive(Debug)]
pub struct Contract {
    /// The task prompt (argv\[1\]).
    pub prompt: String,
    /// The isolated harness root for this run (`LITANY_HOME`).
    pub litany_home: PathBuf,
    /// The experiment `workflow.yaml` to apply (`LITANY_EXPERIMENT`).
    pub experiment: PathBuf,
    /// Where to report back, when the runner wants one
    /// (`LITANY_EVAL_REPORT`).
    pub report: Option<PathBuf>,
    /// The working directory shared with the task `setup` and `check`.
    pub workdir: PathBuf,
}

impl Contract {
    /// Assemble the contract from the raw argv/env material. `Err` is a
    /// human-readable complaint naming what was missing.
    pub fn assemble(
        prompt: Option<String>,
        litany_home: Option<PathBuf>,
        experiment: Option<PathBuf>,
        report: Option<PathBuf>,
        workdir: PathBuf,
    ) -> Result<Self, String> {
        Ok(Self {
            prompt: prompt.ok_or("no prompt on argv[1]")?,
            litany_home: litany_home.ok_or("LITANY_HOME is not set")?,
            experiment: experiment.ok_or("LITANY_EXPERIMENT is not set")?,
            report,
            workdir,
        })
    }
}

/// The machine's litany config root — `$XDG_CONFIG_HOME/litany`, else
/// `~/.config/litany` (ARCH §2.2) — resolved from the given env values,
/// deliberately ignoring `LITANY_HOME`: for the driver that names the
/// per-run isolated home, never the machine's own configuration.
pub fn machine_config_root(
    xdg_config_home: Option<&OsStr>,
    home: Option<&OsStr>,
) -> Option<PathBuf> {
    if let Some(x) = xdg_config_home.filter(|x| !x.is_empty()) {
        return Some(PathBuf::from(x).join("litany"));
    }
    home.filter(|h| !h.is_empty())
        .map(|h| PathBuf::from(h).join(".config").join("litany"))
}

/// The prompt handed to `litany prompt`: the task prompt grounded in
/// the shared working directory. The agent's own shell starts in its
/// worktree (ARCH §3.3 *Working directory*), while the task's `setup`
/// and `check` run in the runner's per-run directory — the driver owns
/// the cwd hand-off, so it names the directory outright.
pub fn grounded(prompt: &str, workdir: &Path) -> String {
    let dir = workdir.display();
    format!(
        "The task's working directory is {dir} — every file the task \
         names lives there, and your shell does not start there. Begin \
         every bash command with `cd {dir} && `.\n\n{prompt}"
    )
}

/// Run the whole driver against a `litany` program: seed, `new`, apply
/// the experiment, `prompt`, report.
pub fn drive(litany: &OsStr, machine_config: Option<&Path>, c: &Contract) -> io::Result<()> {
    if let Some(root) = machine_config {
        seed_home(root, &c.litany_home)?;
    }
    let workspace = non_empty(product(litany_command(litany, c).arg("new"))?, "new")?;
    apply_experiment(litany, c, &workspace)?;
    let agent_id = non_empty(
        product(
            litany_command(litany, c)
                .arg("prompt")
                .arg(&workspace)
                .arg(grounded(&c.prompt, &c.workdir)),
        )?,
        "prompt",
    )?;
    if let Some(report) = &c.report {
        fs::write(report, format!("{workspace}\n{agent_id}\n"))?;
    }
    Ok(())
}

/// Seed the run's isolated home with the machine's wire configuration:
/// `models.yaml` and the `template/` config-root override, when the
/// machine has them. Seed-if-absent, like `litany prime` (ARCH §2.2) —
/// anything already in the run home wins.
fn seed_home(machine_root: &Path, run_home: &Path) -> io::Result<()> {
    fs::create_dir_all(run_home)?;
    let models = machine_root.join("models.yaml");
    let dest = run_home.join("models.yaml");
    if models.is_file() && !dest.exists() {
        fs::copy(&models, &dest)?;
    }
    copy_tree(&machine_root.join("template"), &run_home.join("template"))
}

/// Copy a directory tree, skipping any destination file that already
/// exists (seed-if-absent). A missing source is nothing to seed.
fn copy_tree(src: &Path, dest: &Path) -> io::Result<()> {
    if !src.is_dir() {
        return Ok(());
    }
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let to = dest.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &to)?;
        } else if !to.exists() {
            fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}

/// Apply the experiment into the workspace's config commit (the quiet
/// end of the §9.3 seam): `litany config` materializes the authoring
/// checkout and hands it to `$EDITOR` (as `exec $EDITOR "$1"` through
/// `sh`), so an "editor" of `cp -f "$LITANY_EXPERIMENT"` lands the
/// experiment as the checkout's `workflow.yaml` — its basename, by the
/// runner's resolution — and the commit that follows is the config diff
/// taking force. A declined pass (the baseline's empty diff) exits 0
/// with the branch unmoved: the default is already in force.
fn apply_experiment(litany: &OsStr, c: &Contract, workspace: &str) -> io::Result<()> {
    let mut cmd = litany_command(litany, c);
    cmd.arg("config")
        .arg(workspace)
        .env("EDITOR", "cp -f \"$LITANY_EXPERIMENT\"");
    product(&mut cmd).map(drop)
}

/// A `litany` invocation carrying the run's env: the isolated
/// `LITANY_HOME`, the experiment path (for the `$EDITOR` hand-off),
/// and no inherited `GIT_*` redirection — the driver may itself be
/// running under a git hook, and a leaked `GIT_DIR` silently redirects
/// every git operation the harness performs.
fn litany_command(litany: &OsStr, c: &Contract) -> Command {
    let mut cmd = Command::new(litany);
    cmd.current_dir(&c.workdir)
        .env("LITANY_HOME", &c.litany_home)
        .env("LITANY_EXPERIMENT", &c.experiment);
    for var in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_PREFIX",
        "GIT_COMMON_DIR",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    ] {
        cmd.env_remove(var);
    }
    cmd
}

/// Refuse an empty product from a verb whose product carries the flow
/// (`new`'s workspace path, `prompt`'s agent id).
fn non_empty(product: String, verb: &str) -> io::Result<String> {
    if product.is_empty() {
        Err(io::Error::other(format!(
            "litany {verb} printed no product"
        )))
    } else {
        Ok(product)
    }
}

/// Run one `litany` verb and return its stdout product (ARCH §3.4: one
/// product per verb, trailing newline trimmed). The child's stderr is
/// captured and surfaced only on failure, so 250 quiet runs stay quiet
/// and one broken run says why. The driver never writes to its own
/// stdout — that stream is inherited from the runner.
fn product(cmd: &mut Command) -> io::Result<String> {
    let program = cmd.get_program().to_os_string();
    let out = cmd
        .output()
        .map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", Path::new(&program).display())))?;
    let verb = cmd
        .get_args()
        .next()
        .map(|a| a.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !out.status.success() {
        return Err(io::Error::other(format!(
            "litany {verb} exited {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
}
