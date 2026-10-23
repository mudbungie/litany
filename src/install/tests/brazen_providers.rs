//! Guards the shipped seed data — `template/providers.yaml` (embedded
//! in [`crate::template::TEMPLATE`]) — against brazen's ACTUAL resolved
//! provider table (bl-9391). (`install/models.yaml` names no provider
//! any more — bl-35e2 — so the template roles are the only shipped
//! provider names left to guard.)
//!
//! The drift this pins against: a `provider:` name shipped in the seed
//! file that brazen's pinned build does not resolve surfaces only at an
//! operator's first dispatch ("unknown provider `x`"), never at build or
//! test time — the row's existence is brazen's fact, resolved at call
//! time (ARCH §4.1) — so nothing else in the suite catches a typo'd or
//! renamed row.
//!
//! [`brazen_builtin_provider_names`] answers "what does brazen actually
//! resolve?" by driving brazen's own `run::list_providers` — the engine
//! behind `bz --list-providers` — with the config root forced to an
//! empty temp directory. That keeps the source of truth singular (brazen's
//! table, read through brazen's own code, at litany's exact pinned
//! version) rather than a second list hand-copied into litany that could
//! itself drift when the pin moves; forcing the config root empty keeps
//! the check hermetic — this machine's `~/.config/brazen/config.toml` (an
//! operator's own file, absent in CI) never widens or narrows the answer.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use brazen::{AmbientSpec, Args, Cred, CredStore, EnvSnapshot, ProvidersIo};

use crate::config::per_repo_providers::PerRepoProviders;

/// A `CredStore` that resolves nothing. `list_providers` only asks it
/// whether a credential is stored/discoverable to report the listing's
/// `credential` column — never to authenticate anything — so a store
/// that always answers "no" is exactly as good as a real one here and
/// touches no disk.
struct NullCredStore;

impl CredStore for NullCredStore {
    fn get(&self, _provider: &str) -> Option<Cred> {
        None
    }
    fn put(&self, _provider: &str, _cred: &Cred) -> std::io::Result<()> {
        Ok(())
    }
    fn discover(&self, _spec: &AmbientSpec) -> Option<Cred> {
        None
    }
}

/// Every KEYED provider row name brazen's pinned built-in table resolves,
/// with no operator config in the fold (`XDG_CONFIG_HOME` points at a
/// fresh, empty temp dir, so `read_config_file` finds nothing and the
/// merge bottoms out on `defaults()` alone — the same floor every fresh
/// install gets). Since brazen 0.0.19 (its bl-6feb) the listing is the
/// ROUTING TABLE, not the whole built-in catalog: a preset is listed only
/// once an act reaches it, and the provider-agnostic inline key
/// (`BRAZEN_API_KEY`) is the one act that reaches every keyed preset at
/// once — so with it set the answer is exactly the keyed catalog. A
/// keyless (`ollama`, `claude-code`) or oauth2 preset would need naming;
/// no template role is one, and a role that became one would fail here
/// by design — the guard asks what a fresh install resolves BY KEY.
fn brazen_builtin_provider_names() -> BTreeSet<String> {
    let config_home = tempfile::tempdir().unwrap();
    let mut env = BTreeMap::new();
    env.insert(
        "XDG_CONFIG_HOME".to_string(),
        config_home.path().to_string_lossy().into_owned(),
    );
    // Structured output, so the listing is parsed rather than screen-scraped.
    env.insert("BRAZEN_OUTPUT".to_string(), "ndjson".to_string());
    // The inline key puts every keyed preset in brazen's routing table
    // (brazen config.md §3.2); the value is never sent anywhere.
    env.insert("BRAZEN_API_KEY".to_string(), "notreal".to_string());

    let args = Args {
        argv: Vec::new(),
        env: EnvSnapshot(env),
        tty: false,
        stdout_tty: false,
    };
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut io = ProvidersIo {
        stdout: &mut stdout,
        stderr: &mut stderr,
        store: &NullCredStore,
    };
    let code = brazen::list_providers(&args, &mut io);
    assert_eq!(
        code,
        0,
        "brazen --list-providers over an empty config root failed: {}",
        String::from_utf8_lossy(&stderr)
    );

    let doc: serde_json::Value =
        serde_json::from_slice(&stdout).expect("list-providers json output");
    doc["providers"]
        .as_array()
        .expect("a providers array")
        .iter()
        .map(|row| row["name"].as_str().expect("row has a name").to_owned())
        .collect()
}

/// The `provider:` value of every role the embedded `template/providers.yaml`
/// declares — the file `litany new` authors onto a fresh conversation repo.
fn seeded_role_providers() -> Vec<String> {
    let raw = crate::template::TEMPLATE
        .get_file("providers.yaml")
        .expect("template ships providers.yaml")
        .contents_utf8()
        .expect("providers.yaml is UTF-8");
    let per_repo = PerRepoProviders::parse(raw, Path::new("template/providers.yaml")).unwrap();
    per_repo
        .roles
        .values()
        .map(|a| a.provider.clone())
        .collect()
}

#[test]
fn seeded_providers_yaml_names_only_real_brazen_providers() {
    let known = brazen_builtin_provider_names();
    for provider in seeded_role_providers() {
        assert!(
            known.contains(&provider),
            "template/providers.yaml names provider {provider:?}, which brazen's \
             pinned built-in table does not resolve (known rows: {known:?})"
        );
    }
}
