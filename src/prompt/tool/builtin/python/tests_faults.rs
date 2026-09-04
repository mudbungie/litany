//! The `python` built-in's refusals (`docs/DESIGN_CODE_EXECUTION.md`
//! §2.2, §2.4): a malformed input, an unreadable stdin, a contract var
//! the harness did not set, an agent with no step, a record directory
//! that cannot be made, a committed schema that is not JSON, a relay
//! that will not take the product — and the one failure that is *not* a
//! refusal, a missing interpreter, which is the in-band exit 127. The
//! scene is [`super::tests`]'s.

use super::tests::{AGENT, FakeEnv, Scene, TOOL_ID};
use super::*;
use crate::prompt::tool::inject::InjectedTool;
use serde_json::json;
use std::collections::HashMap;
use std::ffi::OsString;
use std::io::Cursor;

#[test]
fn a_malformed_input_is_refused_before_anything_runs() {
    let scene = Scene::new();
    for bad in [
        b"not json".to_vec(),
        br#"{"other": "x"}"#.to_vec(),
        br#"{"program": "print(1)", "extra": 1}"#.to_vec(),
    ] {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = scene.drive(
            &FakeEnv::at(&scene.ws),
            &mut Cursor::new(bad),
            &mut stdout,
            &mut stderr,
            INTERPRETER,
        );
        assert!(matches!(code, Err(Error::InvalidJson(_))), "{code:?}");
    }
}

/// A `Read` that always fails: stdin is a pipe in production.
struct BrokenReader;

impl Read for BrokenReader {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::other("stdin pipe broken"))
    }
}

/// A `Write` that always fails, for both relay directions.
struct BrokenWriter;

impl Write for BrokenWriter {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("pipe closed"))
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn an_unreadable_stdin_is_refused_rather_than_read_as_an_empty_program() {
    let scene = Scene::new();
    let code = scene.drive(
        &FakeEnv::at(&scene.ws),
        &mut BrokenReader,
        &mut Vec::new(),
        &mut Vec::new(),
        INTERPRETER,
    );
    assert!(matches!(code, Err(Error::StdinRead(_))), "{code:?}");
}

#[test]
fn a_relay_that_cannot_be_written_is_surfaced_rather_than_dropped() {
    let scene = Scene::new();
    let input = json!({ "program": "import sys\nprint('out')\nsys.stderr.write('e')\n" })
        .to_string()
        .into_bytes();
    let out = scene.drive(
        &FakeEnv::at(&scene.ws),
        &mut Cursor::new(input.clone()),
        &mut BrokenWriter,
        &mut Vec::new(),
        INTERPRETER,
    );
    assert!(matches!(out, Err(Error::Stdout(_))), "{out:?}");
    let err = scene.drive(
        &FakeEnv::at(&scene.ws),
        &mut Cursor::new(input),
        &mut Vec::new(),
        &mut BrokenWriter,
        INTERPRETER,
    );
    assert!(matches!(err, Err(Error::Stderr(_))), "{err:?}");
    BrokenWriter.flush().unwrap();
}

#[test]
fn a_missing_or_unreadable_tool_id_is_declined_naming_it() {
    let scene = Scene::new();
    for env in [
        FakeEnv(HashMap::new()),
        FakeEnv(HashMap::from([(
            ENV_TOOL_ID,
            std::os::unix::ffi::OsStringExt::from_vec(vec![0xff, 0xfe]),
        )])),
    ] {
        let (code, _, _) = scene.run_as(&env, "print(1)\n", INTERPRETER);
        let err = code.expect_err("no id, no record directory").to_string();
        assert!(err.contains(ENV_TOOL_ID), "{err}");
    }
}

#[test]
fn an_agent_with_no_step_is_the_caller_s_own_refusal() {
    let scene = Scene::new();
    std::fs::remove_dir_all(&scene.step).unwrap();
    let (code, _, _) = scene.run("print(1)\n");
    let err = code.expect_err("no step, no record").to_string();
    assert!(err.contains("has no step under"), "{err}");
}

#[test]
fn a_record_directory_that_cannot_be_made_surfaces_the_module_write() {
    let scene = Scene::new();
    let tools = scene.step.join(STEP_TOOLS_SUBDIR);
    std::fs::create_dir_all(&tools).unwrap();
    // A file where the record directory belongs: the module has nowhere
    // to land, and a program importing nothing is not run instead.
    std::fs::write(tools.join(TOOL_ID), b"").unwrap();
    let (code, _, _) = scene.run("print(1)\n");
    assert!(matches!(code, Err(Error::Module { .. })), "{code:?}");
}

#[test]
fn a_committed_schema_that_is_not_json_is_a_config_fault_not_a_silent_drop() {
    let scene = Scene::new();
    std::fs::write(
        scene.worktree.join("descriptions/tools").join("bash.json"),
        b"{ not json",
    )
    .unwrap();
    let (code, _, _) = scene.run("print(1)\n");
    assert!(matches!(code, Err(Error::Definitions(_))), "{code:?}");
}

#[test]
fn the_toolset_is_the_grant_plus_the_injection_minus_the_tool_itself() {
    let scene = Scene::new();
    let caller = Caller {
        workspace: scene.ws.clone(),
        agent: AGENT.to_string(),
        step_dir: scene.step.clone(),
        data_root: scene.home.clone(),
        role: "worker".to_string(),
        grant: vec![
            "bash".to_string(),
            super::super::PYTHON.to_string(),
            "granted_but_undescribed".to_string(),
        ],
        injected: vec![
            InjectedTool {
                name: "routed".to_string(),
                description: Some("a host's own".to_string()),
                input_schema: json!({"type": "object"}),
            },
            InjectedTool {
                name: super::super::PYTHON.to_string(),
                description: None,
                input_schema: json!({"type": "object"}),
            },
        ],
        tool_control: None,
    };
    let names: Vec<String> = toolset(&caller)
        .unwrap()
        .into_iter()
        .map(|t| t.name)
        .collect();
    assert_eq!(names, vec!["bash".to_string(), "routed".to_string()]);
}

#[test]
fn the_module_path_is_prepended_to_whatever_pythonpath_already_named() {
    let record = Path::new("/steps/001/tools/tu_1");
    let bare = FakeEnv(HashMap::new());
    assert_eq!(path_with(&bare, record), record.as_os_str());
    let set = FakeEnv(HashMap::from([(PYTHONPATH, OsString::from("/site"))]));
    assert_eq!(
        path_with(&set, record),
        OsString::from("/steps/001/tools/tu_1:/site")
    );
}
