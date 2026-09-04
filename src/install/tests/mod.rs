use super::*;
use crate::harness_root::Roots;
use std::collections::BTreeMap;
use tempfile::TempDir;

/// What the shipped `bash` definition promises the model, split out to
/// keep this file under the repo's per-file line cap.
mod toolspec;

/// What the shipped `python` definition promises the model, split out
/// for the same reason.
mod toolspec_python;

/// The seeded `models.yaml` / `providers.yaml` provider names against
/// brazen's actual resolved table (bl-9391), split out for the same reason.
mod brazen_providers;

/// Pins on the shipped `template/` config — the role grant and the
/// manifest entries a fresh install carries — split out for the same reason.
mod shipped_template;

/// The seeded `learning-loop.yaml` against the basic agentic loop it
/// extends (`docs/DESIGN_LEARNING_LOOP.md` §2), split out for the same
/// reason.
mod learning_loop;

/// The shipped `reviewer` role — soul, grant and manifest entry — split
/// out for the same reason.
mod reviewer_role;

/// `LITANY_HOME`-style collapsed roots: config and data are one directory
/// (ARCH §2.2) — the shape yog drives via `LITANY_HOME=<dir> litany prime`.
fn collapsed(dir: &Path) -> Roots {
    Roots {
        config: dir.to_path_buf(),
        data: dir.to_path_buf(),
    }
}

/// Snapshot every file under `root` as a `relative-path → bytes` map, so
/// two snapshots compare exactly (content and set of files alike).
fn tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(base, &path, out);
            } else {
                let rel = path.strip_prefix(base).unwrap().to_path_buf();
                out.insert(rel, fs::read(&path).unwrap());
            }
        }
    }
    walk(root, root, &mut out);
    out
}

const POOL_ENTRIES: [&str; 7] = [
    "apply_patch",
    "bash",
    "cd",
    "dispatch",
    "load_skill",
    "message",
    "read_file",
];

#[test]
fn seeds_a_fresh_collapsed_home() {
    let home = TempDir::new().unwrap();
    prime(&collapsed(home.path())).unwrap();
    let h = home.path();

    // Config lifetime (§2.2): workflows/ + the default models.yaml (§4.2).
    // The seed is mechanism only (bl-35e2): it must parse as the default
    // shape (no adapter override) and name no model, no provider, and no
    // auth material — model policy never ships in git.
    assert!(h.join("workflows").is_dir());
    let models = fs::read_to_string(h.join("models.yaml")).unwrap();
    let parsed = crate::config::models::Models::load(&h.join("models.yaml")).unwrap();
    assert!(parsed.adapter.is_none(), "the seed activates no override");
    assert!(
        !models.contains("models:"),
        "no models table in the shipped seed (bl-35e2)"
    );
    assert!(
        !models.to_lowercase().contains("claude-"),
        "no model id in the shipped seed (bl-35e2)"
    );
    assert!(
        !models.contains("ANTHROPIC_API_KEY"),
        "auth material must not live in models.yaml (§4.1)"
    );

    // Data lifetime (§2.2): workspaces/ + the tool schema and skill pools.
    assert!(h.join("workspaces").is_dir());
    for name in POOL_ENTRIES {
        assert!(h.join("tools").join(format!("{name}.json")).is_file());
        assert!(h.join("skills").join(name).join("SKILL.md").is_file());
    }
}

#[test]
fn is_idempotent_second_run_changes_nothing() {
    let home = TempDir::new().unwrap();
    let roots = collapsed(home.path());
    prime(&roots).unwrap();

    let before = tree(home.path());
    let models_mtime = fs::metadata(home.path().join("models.yaml"))
        .unwrap()
        .modified()
        .unwrap();

    prime(&roots).unwrap();

    assert_eq!(before, tree(home.path()), "a re-prime changed the tree");
    assert_eq!(
        models_mtime,
        fs::metadata(home.path().join("models.yaml"))
            .unwrap()
            .modified()
            .unwrap(),
        "models.yaml was rewritten by a re-prime"
    );
}

#[test]
fn does_not_clobber_hand_edited_models_yaml() {
    let home = TempDir::new().unwrap();
    let roots = collapsed(home.path());
    prime(&roots).unwrap();

    let models = home.path().join("models.yaml");
    fs::write(&models, "models: {}\n").unwrap();
    prime(&roots).unwrap();

    assert_eq!(fs::read_to_string(&models).unwrap(), "models: {}\n");
}

#[test]
fn does_not_clobber_pool_entries_and_keeps_extras() {
    let home = TempDir::new().unwrap();
    let roots = collapsed(home.path());
    prime(&roots).unwrap();

    // A hand-edited shipped skill, an extra file inside a shipped skill
    // dir, and an operator-added tool schema must all survive a re-prime
    // (seed-if-absent, never clobber; §2.2).
    let skill = home.path().join("skills/bash/SKILL.md");
    fs::write(&skill, "edited\n").unwrap();
    fs::write(home.path().join("skills/bash/extra.md"), "mine\n").unwrap();
    fs::write(home.path().join("tools/custom.json"), "{}\n").unwrap();

    prime(&roots).unwrap();

    assert_eq!(fs::read_to_string(&skill).unwrap(), "edited\n");
    assert!(home.path().join("skills/bash/extra.md").is_file());
    assert!(home.path().join("tools/custom.json").is_file());
}

#[test]
fn split_roots_place_config_and_data_apart() {
    // Under XDG (no LITANY_HOME) the roots diverge; prime must route the
    // config artifacts to `config` and the pools to `data` (§2.2).
    let cfg = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let roots = Roots {
        config: cfg.path().to_path_buf(),
        data: data.path().to_path_buf(),
    };
    prime(&roots).unwrap();

    assert!(cfg.path().join("models.yaml").is_file());
    assert!(cfg.path().join("workflows").is_dir());
    assert!(!cfg.path().join("tools").exists());

    assert!(data.path().join("tools/bash.json").is_file());
    assert!(data.path().join("skills/bash/SKILL.md").is_file());
    assert!(data.path().join("workspaces").is_dir());
    assert!(!data.path().join("models.yaml").exists());
}

#[test]
fn seed_file_skips_an_existing_path() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("f");
    fs::write(&path, b"orig").unwrap();
    seed_file(&path, b"new").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"orig");
}

#[test]
fn seed_file_writes_when_absent() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("f");
    seed_file(&path, b"new").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"new");
}

#[test]
fn seed_file_write_error_surfaces_io() {
    let tmp = TempDir::new().unwrap();
    // A regular file as the parent makes the write to `<file>/child` fail
    // with a non-`AlreadyExists` error, surfaced as `Error::Io`.
    let blocker = tmp.path().join("blocker");
    fs::write(&blocker, b"x").unwrap();
    let err = seed_file(&blocker.join("child"), b"data").unwrap_err();
    assert!(matches!(err, Error::Io { .. }), "got {err:?}");
    assert!(err.to_string().contains("prime I/O"));
}

#[test]
fn ensure_dir_error_surfaces_io() {
    let tmp = TempDir::new().unwrap();
    let blocker = tmp.path().join("blocker");
    fs::write(&blocker, b"x").unwrap();
    let err = ensure_dir(&blocker.join("under")).unwrap_err();
    assert!(matches!(err, Error::Io { .. }), "got {err:?}");
}

#[test]
fn prime_surfaces_a_pool_seed_error() {
    // A regular file where the tools pool dir must be created makes
    // `extract_dir`'s `ensure_dir` fail — the error escapes prime.
    let home = TempDir::new().unwrap();
    fs::write(home.path().join("tools"), b"not a dir").unwrap();
    let err = prime(&collapsed(home.path())).unwrap_err();
    assert!(matches!(err, Error::Io { .. }), "got {err:?}");
}

/// `litany prime` validates what it seeds: the embedded pools it ships —
/// `schemas/tools/**` and `skills/**` — must parse with the same parsers
/// [`crate::template::descriptions::snapshot`] now runs at snapshot time
/// (bl-e3f5). This is the cheap insurance ARCH §2.2 asks for: a shipped
/// asset that fails to parse would refuse *every* `litany new` and
/// `litany config` on a fresh install, so it must never reach a release.
#[test]
fn shipped_skill_pool_frontmatter_parses() {
    for sub in SKILLS.dirs() {
        let manifest = sub
            .files()
            .find(|f| f.path().file_name().and_then(|n| n.to_str()) == Some("SKILL.md"))
            .unwrap_or_else(|| panic!("{:?} ships no SKILL.md", sub.path()));
        let raw = manifest
            .contents_utf8()
            .unwrap_or_else(|| panic!("{:?} is not UTF-8", manifest.path()));
        let body = crate::skill::frontmatter_yaml(raw)
            .unwrap_or_else(|| panic!("{:?} has no frontmatter block", manifest.path()));
        crate::skill::parse(body)
            .unwrap_or_else(|e| panic!("{:?} frontmatter is malformed: {e}", manifest.path()));
    }
}

/// The shipped tool schema pool parses as JSON — same insurance as
/// [`shipped_skill_pool_frontmatter_parses`], for the other artifact kind
/// `descriptions::snapshot` validates (bl-e3f5).
#[test]
fn shipped_tool_schema_pool_parses() {
    for file in TOOLS.files() {
        serde_json::from_slice::<serde_json::Value>(file.contents())
            .unwrap_or_else(|e| panic!("{:?} is not valid JSON: {e}", file.path()));
    }
}

#[test]
fn prime_surfaces_a_config_seed_error() {
    // The config root itself is a regular file → `ensure_dir` of the
    // `workflows/` subdir fails on the first config-lifetime step.
    let holder = TempDir::new().unwrap();
    let cfg_file = holder.path().join("cfg");
    fs::write(&cfg_file, b"x").unwrap();
    let data = TempDir::new().unwrap();
    let roots = Roots {
        config: cfg_file,
        data: data.path().to_path_buf(),
    };
    let err = prime(&roots).unwrap_err();
    assert!(matches!(err, Error::Io { .. }), "got {err:?}");
}
