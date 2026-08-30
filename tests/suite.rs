//! Well-formedness gate for the evaluation task suite (ARCH §9.1, v0.9).
//!
//! The suite itself is data under `tests/suite/` (one YAML file per failure
//! category). The runner that executes it (`crates/agent-eval`, §9.3) reads
//! this directory through `agent_eval::suite`, and the shipped harness
//! driver (`crates/litany-eval-agent`) closes the loop to a live model —
//! `make eval` runs this data end to end. This test pins the structural
//! contract the runner relies on: 50 uniquely-identified tasks, each with
//! a prompt and a
//! machine-checkable `check`, tagged only with the seven §9.1 categories, its
//! file's category as its primary tag, and — the §9.1 statistical-power
//! target — at least ten tasks per category (reached via secondary tags,
//! since seven categories cannot each hold ten of fifty tasks disjointly).

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::PathBuf;

/// The seven §9.1 failure categories, each the stem of its suite file.
const CATEGORIES: [&str; 7] = [
    "early_termination",
    "scope_reduction",
    "skipped_tests",
    "hallucinated_apis",
    "error_recovery",
    "fabricated_success",
    "context_hygiene",
];

const TOTAL_TASKS: usize = 50;
const MIN_PER_CATEGORY: usize = 10;

#[derive(serde::Deserialize)]
struct Task {
    id: String,
    categories: Vec<String>,
    prompt: String,
    check: String,
    #[serde(default)]
    setup: Option<String>,
}

#[derive(serde::Deserialize)]
struct SuiteFile {
    tasks: Vec<Task>,
}

fn suite_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/suite")
}

#[test]
fn suite_is_well_formed() {
    let valid: BTreeSet<&str> = CATEGORIES.into_iter().collect();
    let mut ids: BTreeSet<String> = BTreeSet::new();
    let mut per_category: BTreeMap<&str, usize> = CATEGORIES.iter().map(|c| (*c, 0)).collect();
    let mut total = 0usize;

    for primary in CATEGORIES {
        let path = suite_dir().join(format!("{primary}.yaml"));
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let file: SuiteFile = serde_yaml_ng::from_str(&text)
            .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));

        for task in file.tasks {
            total += 1;
            // Unique id across the whole suite.
            assert!(ids.insert(task.id.clone()), "duplicate task id {}", task.id);
            // A prompt and a machine-checkable criterion, both non-empty.
            assert!(!task.prompt.trim().is_empty(), "{}: empty prompt", task.id);
            assert!(!task.check.trim().is_empty(), "{}: empty check", task.id);
            // An optional setup, non-empty when present.
            if let Some(setup) = &task.setup {
                assert!(!setup.trim().is_empty(), "{}: empty setup", task.id);
            }
            // At least one category; the first is the primary and equals the
            // file's category (file placement is the single source of truth).
            assert!(!task.categories.is_empty(), "{}: no categories", task.id);
            assert_eq!(
                task.categories[0], primary,
                "{}: primary tag must match its file",
                task.id
            );
            for tag in &task.categories {
                assert!(
                    valid.contains(tag.as_str()),
                    "{}: unknown tag {tag}",
                    task.id
                );
                *per_category.get_mut(tag.as_str()).unwrap() += 1;
            }
        }
    }

    assert_eq!(
        total, TOTAL_TASKS,
        "suite must hold exactly {TOTAL_TASKS} tasks"
    );
    for cat in CATEGORIES {
        let n = per_category[cat];
        assert!(
            n >= MIN_PER_CATEGORY,
            "category {cat} has {n} tasks (<{MIN_PER_CATEGORY}, §9.1)"
        );
    }
}
