//! The `search_history` declines (ARCH §3.3): every way the tool refuses,
//! each reaching the model as an `is_error` `tool_result` carrying the
//! reason verbatim. Split from [`super`] for the per-file line cap.

use super::*;

#[test]
fn both_inputs_or_neither_is_declined_naming_the_two_shapes() {
    let (holder, _repo) = workspace_repo();
    for input in [
        serde_json::json!({}),
        serde_json::json!({"pattern": "a", "entry": "b"}),
    ] {
        let err = decline(holder.path(), &input);
        assert!(matches!(err, Error::Ambiguous), "{err}");
        assert!(err.to_string().contains("exactly one"), "{err}");
    }
}

#[test]
fn an_unknown_field_is_invalid_json() {
    let (holder, _repo) = workspace_repo();
    let err = decline(
        holder.path(),
        &serde_json::json!({"pattern": "a", "limit": 3}),
    );
    assert!(matches!(err, Error::InvalidJson(_)), "{err}");
    assert!(err.to_string().starts_with("invalid input JSON"), "{err}");
}

#[test]
fn a_broken_stdin_is_its_own_variant() {
    let (holder, _repo) = workspace_repo();
    let err = run(&mut FailingReader, &mut Vec::new(), &env(holder.path())).unwrap_err();
    assert!(matches!(err, Error::StdinRead(_)), "{err}");
    assert!(err.to_string().contains("stdin boom"), "{err}");
}

#[test]
fn a_missing_workspace_env_var_is_declined() {
    let mut stdin = Cursor::new(br#"{"pattern":"x"}"#.to_vec());
    let err = run(&mut stdin, &mut Vec::new(), &StubEnv(HashMap::new())).unwrap_err();
    assert!(matches!(err, Error::MissingEnv(ENV_CONV_REPO)), "{err}");
    assert!(err.to_string().contains("LITANY_CONV_REPO"), "{err}");
}

#[test]
fn an_address_naming_no_blob_surfaces_gits_own_decline() {
    let (holder, _repo) = workspace_repo();
    let err = decline(
        holder.path(),
        &serde_json::json!({"entry": "HEAD:no/such.md"}),
    );
    assert!(
        matches!(&err, Error::Git { op, .. } if *op == "search_history show"),
        "{err}"
    );
    assert!(err.to_string().starts_with("search_history show:"), "{err}");
}

#[test]
fn an_unreadable_workspace_surfaces_the_log_failure() {
    let holder = TempDir::new().unwrap();
    let mut stdin = Cursor::new(br#"{"pattern":"x"}"#.to_vec());
    let err = run(&mut stdin, &mut Vec::new(), &env(holder.path())).unwrap_err();
    assert!(
        matches!(&err, Error::Git { op, .. } if *op == "search_history log"),
        "{err}"
    );
    assert!(err.to_string().starts_with("search_history log:"), "{err}");
}

#[test]
fn a_broken_stdout_is_its_own_variant() {
    let (holder, repo) = workspace_repo();
    commit(&repo, "step 002", &[("summary/001.md", "needle\n")], &[]);
    let mut stdin = Cursor::new(br#"{"pattern":"needle"}"#.to_vec());
    let err = run(&mut stdin, &mut FailingWriter, &env(holder.path())).unwrap_err();
    assert!(matches!(err, Error::Write(_)), "{err}");
    assert!(err.to_string().contains("stdout boom"), "{err}");
    // The double is deliberate: a writer that failed only on `write`
    // would let a buffered caller through on the flush.
    assert!(FailingWriter.flush().is_err());
}
