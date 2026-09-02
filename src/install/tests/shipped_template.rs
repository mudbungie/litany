//! Pins on the **shipped template** (`template/`): the config a fresh
//! install actually carries. Split out of `mod.rs` to keep that file
//! under the repo's per-file line cap.

use super::*;

/// The shipped `worker` grant is the shipped tool pool (yog bl-bd9d).
///
/// ARCH §4.3: *"A root records no role and resolves the `worker` default
/// — roots are workers"*. So `template/providers.yaml`'s `worker` row is
/// what every interactive conversation can call out of the box, and the
/// pool seeded above is this install's own declaration of what it
/// provides. A row granting a *subset* of the pool has no principle
/// behind it, only drift: the shipped row read `[bash, read_file,
/// load_skill]` while the pool shipped `message` and `dispatch` too, so
/// no root agent in any workspace could message a sibling or dispatch a
/// child — twice diagnosed live as a model fault before the config gap
/// was found. Scrutiny, not mechanism: the list stays authored in the
/// template (visible, overridable, severable) and this test is what
/// keeps it from silently falling behind the pool again.
#[test]
fn the_shipped_worker_grant_is_the_whole_tool_pool() {
    let raw = crate::template::TEMPLATE
        .get_file("providers.yaml")
        .expect("the template ships providers.yaml")
        .contents_utf8()
        .expect("providers.yaml is UTF-8");
    let shipped = crate::config::PerRepoProviders::parse(raw, Path::new("template/providers.yaml"))
        .expect("the shipped template parses");

    let mut granted = shipped.roles["worker"].tools.clone();
    granted.sort();
    let mut pool: Vec<String> = TOOLS
        .files()
        .map(|f| {
            f.path()
                .file_stem()
                .expect("a pooled schema has a stem")
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    pool.sort();

    assert_eq!(
        granted, pool,
        "template/providers.yaml's worker `tools:` must grant every tool \
         schemas/tools/ ships (ARCH §4.3)"
    );
    // The compactor is the deliberate exception: its pair is injected by
    // the compaction procedure and is never declarable (§2.7, §4.3).
    assert!(shipped.roles["compactor"].tools.is_empty());
}

/// The shipped `compactor` manifest entry composes what the compactor's
/// goal tells it to read (bl-2c63).
///
/// The grant above is empty by design, so the manifest entry is the
/// compactor's *only* route to anything but the unconditional transcript
/// tail (§5.1: the tree bounds, the manifest selects). The summary chain
/// composes here or nowhere — the compaction landing admits only the
/// summary and the deletions (§2.6), so no prior compactor's reasoning
/// ever lands in the dispatching branch's transcript — and a summary the
/// next compactor cannot see is one it destroys when it supersedes it.
/// The shipped entry read `order: []` while the boilerplate goal told it
/// to read `summary/`, which is the defect this pins shut. Work products
/// stay out: no honest glob names them, and the transcript already
/// carries the acts that produced them.
#[test]
fn the_shipped_compactor_entry_composes_the_summary_chain() {
    let raw = crate::template::TEMPLATE
        .get_file("manifest.yaml")
        .expect("the template ships manifest.yaml")
        .contents_utf8()
        .expect("manifest.yaml is UTF-8");
    let shipped =
        crate::config::manifest::Manifest::parse(raw, Path::new("template/manifest.yaml"))
            .expect("the shipped template parses");
    assert_eq!(shipped.roles["compactor"].order, vec!["summary/**"]);
}

/// The named default exists in the pool config commits are authored from
/// (ARCH §2.2, §6): `prime` seeds `workflows/basic-agentic-loop.yaml`.
///
/// The pool was founded empty, so the **basic agentic loop** — the default
/// the 2026-08-31 ruling named, and the declaration every `config/default`
/// freezes — had no file there to read, copy or fork a variant from; an
/// operator authoring an alternative workflow had nothing to start from but
/// a workspace checkout. The entry is not a second declaration: both
/// seeding paths read the one embedded `template/workflow.yaml`, so the
/// pool's default and the freeze cannot disagree. Seed-if-absent like every
/// other entry, so a curated file survives a re-prime (§2.2).
#[test]
fn prime_seeds_the_basic_agentic_loop_into_the_workflow_pool() {
    let home = TempDir::new().unwrap();
    prime(&collapsed(home.path())).unwrap();

    let seeded = home.path().join(WORKFLOWS_DIR).join(BASIC_AGENTIC_LOOP);
    let template = crate::template::TEMPLATE
        .get_file("workflow.yaml")
        .expect("the template ships workflow.yaml")
        .contents();
    assert_eq!(fs::read(&seeded).unwrap(), template);

    fs::write(&seeded, "events: {}\n").unwrap();
    prime(&collapsed(home.path())).unwrap();
    assert_eq!(fs::read_to_string(&seeded).unwrap(), "events: {}\n");
}
