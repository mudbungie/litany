//! Adapter-resolution variants of the happy path (ARCH §4.2, §4.4):
//! a `models.yaml` `adapter:` override and a binding-injected adapter
//! target each skip the load-time version guard and are invoked
//! verbatim. Split from [`super::happy`] for the 300-line cap.

use super::fixtures::*;
use crate::prompt::run;
use std::ffi::OsStr;

#[test]
fn run_under_adapter_override_skips_version_guard_and_uses_the_override() {
    // With an `adapter:` override in models.yaml the version guard is
    // skipped (§4.2); the MessageStart.v handshake governs. The stub
    // adapter is scripted with just the model stream — no `--version`.
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let harness = scaffold_harness_root_with_adapter("/opt/alt-bz");
    let adapter = StubAdapter::scripted([StubAdapter::reply_ok(&happy_response_bytes())]);
    let git = StubGit::ok();
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let (sleeper, tool_executor) = (StubSleeper::default(), StubToolExecutor::ok());

    run(
        repo.path(),
        "hi",
        None,
        None,
        None,
        crate::prompt::PinnedDocs::none(),
        None,
        None,
        &valid_deps(
            &adapter,
            &sleeper,
            &git,
            &clock,
            &id,
            &tool_executor,
            harness.path(),
        ),
    )
    .unwrap();

    // Exactly one adapter call — the model call — against the override
    // binary; no `--version` guard call.
    let invocations = adapter.observed.borrow().clone();
    assert_eq!(invocations.len(), 1);
    assert_eq!(invocations[0].0, OsStr::new("/opt/alt-bz"));
    assert_eq!(invocations[0].1, vec!["--json", "--provider", "anthropic"]);
}

#[test]
fn run_under_injected_adapter_target_skips_version_guard_and_uses_the_target() {
    // A binding-injected adapter target (`cmd::Fx::adapter_target`, §3.4) —
    // an embedding host naming itself as the provider adapter — is used
    // verbatim and, like a `models.yaml` `adapter:` override, skips the
    // load-time version guard; the MessageStart.v handshake governs (§4.4).
    // There is no `adapter:` override here, so the injection is the sole
    // named target — the host-asserted-identity case.
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let harness = scaffold_harness_root();
    let adapter = StubAdapter::scripted([StubAdapter::reply_ok(&happy_response_bytes())]);
    let git = StubGit::ok();
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let (sleeper, tool_executor) = (StubSleeper::default(), StubToolExecutor::ok());

    let injected = std::path::PathBuf::from("/opt/host-bz");
    let mut deps = valid_deps(
        &adapter,
        &sleeper,
        &git,
        &clock,
        &id,
        &tool_executor,
        harness.path(),
    );
    deps.adapter_target = Some(&injected);
    run(
        repo.path(),
        "hi",
        None,
        None,
        None,
        crate::prompt::PinnedDocs::none(),
        None,
        None,
        &deps,
    )
    .unwrap();

    // Exactly one adapter call — the model call — against the injected
    // target; no `--version` guard call.
    let invocations = adapter.observed.borrow().clone();
    assert_eq!(invocations.len(), 1);
    assert_eq!(invocations[0].0, OsStr::new("/opt/host-bz"));
    assert_eq!(invocations[0].1, vec!["--json", "--provider", "anthropic"]);
}
