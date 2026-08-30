//! The one interactive sliver of the exec binding that lives at the
//! coverage-exempt bin seam (ARCH §3.4): the `litany config` `$EDITOR`
//! hand-off, injected as [`litany::cmd::Fx::editor`]. A real interactive
//! `$EDITOR` session cannot be driven from a test; the decline path can,
//! and is (below) — origin resolution and the commit are covered where
//! they actually live, in the crate's private `template::authoring`
//! machinery. The `litany advance` successor `exec` is no longer a
//! bespoke handler: it rides the generic [`litany::cmd::Outcome::Exec`]
//! the binding performs in `main`.

use std::io;
use std::path::Path;

/// The `litany config` `$EDITOR` hand-off (ARCH §2.2, §3.4): open the
/// authoring checkout so the user edits the control files, treating a
/// non-zero editor exit as a failed edit. `$EDITOR` may carry arguments,
/// so it runs through `sh -c`.
pub(crate) fn edit_in_editor(dir: &Path) -> io::Result<()> {
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("exec {editor} \"$1\""))
        .arg("sh")
        .arg(dir)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        let reason = match status.code() {
            // `sh -c 'exec ...'` reports 127 when the shell could not find
            // the command at all — the common "$EDITOR is misconfigured"
            // case, so name it instead of a bare exit code.
            Some(127) => format!("editor \"{editor}\" not found on PATH"),
            Some(code) => format!("editor \"{editor}\" exited with exit status {code}"),
            None => format!("editor \"{editor}\" exited with {status}"),
        };
        Err(io::Error::other(format!(
            "{reason} — set $EDITOR to a working editor and retry"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both failure shapes named at the repro (bl-79ea): a script that
    /// exits non-zero, and an `$EDITOR` that is not a command at all. One
    /// test, run sequentially, so the two `EDITOR` mutations never race a
    /// sibling test's read of the same process-wide var.
    #[test]
    fn declined_editor_names_the_command_and_the_knob() {
        let dir = tempfile::TempDir::new().unwrap();

        // SAFETY: no other thread in this test binary reads or writes
        // EDITOR; this is the only test in the crate that touches it.
        unsafe { std::env::set_var("EDITOR", "false") };
        let err = edit_in_editor(dir.path()).unwrap_err();
        assert_eq!(
            err.to_string(),
            "editor \"false\" exited with exit status 1 — set $EDITOR to a working editor and retry"
        );

        unsafe { std::env::set_var("EDITOR", "/no/such/editor-binary") };
        let err = edit_in_editor(dir.path()).unwrap_err();
        assert_eq!(
            err.to_string(),
            "editor \"/no/such/editor-binary\" not found on PATH — set $EDITOR to a working editor and retry"
        );

        unsafe { std::env::remove_var("EDITOR") };
    }
}
