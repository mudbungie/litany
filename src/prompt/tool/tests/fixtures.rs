//! Shared test fixtures. Helpers lay down executable shell scripts in
//! a tempdir-rooted harness root so [`super::super::SpawnTool`] can
//! resolve them via the §3.3 lookup order without touching `PATH`.

use crate::prompt::Clock;
use std::cell::RefCell;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// The injected driver target (`cmd::Fx::driver_target`, §2.11) for
/// tests whose tool resolves at the first or second §3.3 hop, so the
/// third is never consulted. A bare name makes a regression that *does*
/// reach the third hop fail loudly at spawn rather than quietly
/// re-entering the test binary — which is exactly what threading the
/// target instead of `current_exe` bought.
pub(super) fn driver_target() -> &'static Path {
    Path::new("litany")
}

/// The bytes a §3.3 *result envelope* carries after its `Exit code: N`
/// header — for tests whose subject is what the tool printed rather than
/// how the result is framed. The framing itself is pinned by
/// `super::super::envelope`'s own tests and, end to end, by
/// [`super::happy`]; re-stating it in every unrelated assertion would
/// make one shape twenty places to edit.
pub(super) fn after_header(content: &[u8]) -> &[u8] {
    let header_end = content
        .iter()
        .position(|byte| *byte == b'\n')
        .expect("a result envelope opens with its exit-code line");
    &content[header_end + 1..]
}

/// Deterministic [`Clock`] — `started_at` / `ended_at` come back as
/// `iso-1` and `iso-2` so the on-disk record's timestamps are
/// observable in assertions without dragging the wall clock in.
#[derive(Default)]
pub(super) struct FixedClock {
    iso_calls: RefCell<u32>,
}

impl Clock for FixedClock {
    fn now_iso8601(&self) -> String {
        *self.iso_calls.borrow_mut() += 1;
        format!("iso-{}", self.iso_calls.borrow())
    }
    fn now_compact(&self) -> String {
        // Unused by the executor but the trait demands it.
        "ct".into()
    }
}

/// Harness root containing a `tools/` subdir; the [`TempDir`] is held
/// so its lifetime spans the whole test. Tests interact with it
/// through [`Self::install`] which drops a script into
/// `tools/litany-tool-<name>`.
pub(super) struct HarnessRoot {
    pub(super) dir: TempDir,
}

impl HarnessRoot {
    pub(super) fn new() -> Self {
        let dir = TempDir::new().expect("tempdir");
        std::fs::create_dir_all(dir.path().join(super::super::TOOLS_DIR)).expect("mkdir tools/");
        Self { dir }
    }

    pub(super) fn path(&self) -> &Path {
        self.dir.path()
    }

    /// Drop a chmod-+x shell script under
    /// `<root>/tools/litany-tool-<name>` and return its absolute path.
    pub(super) fn install(&self, name: &str, body: &str) -> PathBuf {
        let path = self.dir.path().join(super::super::TOOLS_DIR).join(format!(
            "{}{}",
            super::super::EXTERNAL_PREFIX,
            name
        ));
        write_script(&path, body);
        path
    }
}

/// Write `body` to `path`, prepend the bash shebang, and chmod 0o755
/// so the kernel will exec it. Used both by the harness-root installer
/// and by tests that need a binary outside the harness root.
pub(super) fn write_script(path: &Path, body: &str) {
    let mut script = String::from("#!/usr/bin/env bash\n");
    script.push_str(body);
    if !script.ends_with('\n') {
        script.push('\n');
    }
    std::fs::write(path, script).expect("write script");
    let mut perm = std::fs::metadata(path).expect("stat").permissions();
    perm.set_mode(0o755);
    std::fs::set_permissions(path, perm).expect("chmod");
}

/// Per-test workspace slice. Mirrors the v0.3.1 layout — the step
/// directory `<workspace>/steps/<agent-id>/<NNN>/` (ARCH §2.2 / §2.3 —
/// at the workspace root, outside every worktree) so the executor lands
/// `tools/<tool-id>/` underneath, plus the calling agent's worktree at
/// `<workspace>/agents/<agent-id>/`, which the executor derives from the
/// step dir and runs every tool subprocess in (§3.3 *Working
/// directory*). `_root` is held only for its `Drop` — the tempdir
/// cleanup happens when [`StepDir`] goes out of scope.
pub(super) struct StepDir {
    _root: TempDir,
    pub(super) path: PathBuf,
    pub(super) worktree: PathBuf,
}

/// The agent id every [`StepDir`] is namespaced under.
pub(super) const AGENT_ID: &str = "convid";

impl StepDir {
    pub(super) fn new() -> Self {
        let root = TempDir::new().expect("step tempdir");
        let path = root.path().join("steps").join(AGENT_ID).join("001");
        std::fs::create_dir_all(&path).expect("mkdir step");
        let worktree = crate::workspace::agent_worktree(root.path(), AGENT_ID);
        std::fs::create_dir_all(&worktree).expect("mkdir worktree");
        Self {
            _root: root,
            path,
            worktree,
        }
    }
}
