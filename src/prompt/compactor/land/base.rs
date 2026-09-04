//! The **compaction base** (ARCH §2.6): one commit whose tree is the
//! compaction point's with the product applied, parented on the span's
//! lower bound — the squash the landing rebases the live tail onto.
//!
//! The product is classified from the compactor's branch **after its own
//! dispatch commit**: a path *deleted* in `dispatch..tip` is a
//! `mark_for_deletion` nomination (the fork-time prunes — the empty-grant
//! `descriptions/**` derivation, the unsettled-tool-step removal — all
//! land *on* the dispatch commit and are thereby excluded, structurally);
//! a path *added under `summary/`* is the `write_summary` product.
//! Nothing else exists to a landing: the compactor's dialog, goal, and
//! soul are additions outside `summary/` and rewrites, which this module
//! never reads.
//!
//! The base is minted without disturbing the live checkout: a throwaway
//! `--no-checkout` worktree gives us a private index (no `GIT_INDEX_FILE`
//! plumbing, no worktree churn) — `read-tree <P>`, drop the deletions,
//! stage the summary blobs from the compactor's tip, `write-tree`,
//! `commit-tree -p <bound>`. Pure object-store writes; the branch ref
//! does not move here (the replay moves it,
//! [`crate::prompt::rebase_forward`]).

use super::super::{Error, checkpoint};
use super::extract::{self, Extract};
use super::span::Span;
use crate::template::GitRunner;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// The compaction product: what the compactor's two tools committed after
/// its dispatch commit (module docs), plus the one product no model
/// authors — the **extract** the landing itself derives (docs/TAXONOMY.md
/// §3, [`extract`]) — and nothing else.
pub(super) struct Product {
    /// Paths nominated by `mark_for_deletion` — deleted in
    /// `dispatch..tip` on the compactor's branch.
    pub(super) deletions: Vec<String>,
    /// `summary/**` paths added by `write_summary`.
    pub(super) summaries: Vec<String>,
    /// `summary/<NNN>.refs.md`, derived here from what the deletions take
    /// out of context; `None` when the workflow declares no
    /// `extract_bytes`, when no summary was written for it to sit beside,
    /// or when nothing referable was removed ([`extract::of`]).
    pub(super) extract: Option<Extract>,
}

impl Product {
    /// No deletions and no summary: nothing to land ([`super::LandOutcome::NoOp`]).
    pub(super) fn is_empty(&self) -> bool {
        self.deletions.is_empty() && self.summaries.is_empty()
    }
}

/// Classify the compaction product from the compactor's branch (module
/// docs): deletions and `summary/**` additions in `dispatch..tip`, then
/// the extract derived from the first of those two.
pub(super) fn product(
    parent_worktree: &Path,
    span: &Span,
    compactor_ref: &str,
    extract_bytes: Option<usize>,
    git: &dyn GitRunner,
) -> Result<Product, Error> {
    let dispatch = span.dispatch.as_str();
    let deletions = diff_class(parent_worktree, dispatch, compactor_ref, "D", None, git)?;
    let summaries = diff_class(
        parent_worktree,
        dispatch,
        compactor_ref,
        "A",
        Some("summary"),
        git,
    )?;
    let extract = extract::of(
        parent_worktree,
        span,
        &deletions,
        &summaries,
        extract_bytes,
        git,
    )?;
    Ok(Product {
        deletions,
        summaries,
        extract,
    })
}

/// Paths of one `--diff-filter` class between two trees, optionally
/// limited to a pathspec. `--no-renames` keeps the classes exhaustive: an
/// add/delete pair must not collapse into an `R` that escapes both.
fn diff_class(
    parent_worktree: &Path,
    from: &str,
    to: &str,
    class: &str,
    pathspec: Option<&str>,
    git: &dyn GitRunner,
) -> Result<Vec<String>, Error> {
    let filter = format!("--diff-filter={class}");
    let mut args = vec![
        "diff",
        "--name-only",
        "--no-renames",
        filter.as_str(),
        from,
        to,
    ];
    if let Some(spec) = pathspec {
        args.extend(["--", spec]);
    }
    let out = git
        .run_capture(parent_worktree, &args)
        .map_err(|source| Error::Git {
            op: "compaction land product diff",
            source,
        })?;
    Ok(out
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect())
}

/// Mint the compaction base commit (module docs) and return its sha.
pub(super) fn commit(
    parent_worktree: &Path,
    compactor_id: &str,
    compactor_ref: &str,
    span: &Span,
    product: &Product,
    git: &dyn GitRunner,
) -> Result<String, Error> {
    let tmp = scratch_worktree(compactor_id);
    let tmp_str = tmp.to_string_lossy().into_owned();
    git.run(
        parent_worktree,
        &[
            "worktree",
            "add",
            "--no-checkout",
            "--detach",
            &tmp_str,
            &span.point,
        ],
    )
    .map_err(|source| Error::Git {
        op: "compaction land scratch worktree",
        source,
    })?;
    let minted = mint(&tmp, compactor_id, compactor_ref, span, product, git);
    // The scratch worktree is disposable either way; a removal failure
    // must not shadow the mint's own outcome. The directory goes too —
    // the extract is staged from a file written into it (below), and a
    // half-removed scratch tree is nobody's to read.
    let _ = git.run(
        parent_worktree,
        &["worktree", "remove", "--force", &tmp_str],
    );
    let _ = std::fs::remove_dir_all(&tmp);
    minted
}

/// The object-store half of [`commit`], run inside the scratch worktree's
/// private index.
fn mint(
    tmp: &Path,
    compactor_id: &str,
    compactor_ref: &str,
    span: &Span,
    product: &Product,
    git: &dyn GitRunner,
) -> Result<String, Error> {
    let err = |op| move |source| Error::Git { op, source };
    git.run(tmp, &["read-tree", &span.point])
        .map_err(err("compaction land read-tree"))?;
    if !product.deletions.is_empty() {
        let mut args = vec!["rm", "--cached", "-q", "--ignore-unmatch", "--"];
        args.extend(product.deletions.iter().map(String::as_str));
        git.run(tmp, &args)
            .map_err(err("compaction land apply deletions"))?;
    }
    if !product.summaries.is_empty() {
        let source_arg = format!("--source={compactor_ref}");
        let mut args = vec!["restore", "--staged", source_arg.as_str(), "--"];
        args.extend(product.summaries.iter().map(String::as_str));
        git.run(tmp, &args)
            .map_err(err("compaction land stage summary"))?;
    }
    if let Some(extract) = &product.extract {
        // `git add` copies the blob into the object store and stages it
        // there and then, so the file need not survive `write-tree` —
        // and the scratch tree, checked out with `--no-checkout`, keeps
        // nothing else on disk.
        let abs = tmp.join(&extract.path);
        std::fs::create_dir_all(abs.parent().expect("summary/ has a parent"))?;
        std::fs::write(&abs, &extract.text)?;
        git.run(tmp, &["add", "--", &extract.path])
            .map_err(err("compaction land stage extract"))?;
    }
    let tree = git
        .run_capture(tmp, &["write-tree"])
        .map_err(err("compaction land write-tree"))?;
    let subject = format!("{}{compactor_id}]", checkpoint::BASE_SUBJECT_PREFIX);
    let sha = git
        .run_capture(
            tmp,
            &[
                "commit-tree",
                tree.trim(),
                "-p",
                &span.bound,
                "-m",
                &subject,
            ],
        )
        .map_err(err("compaction land commit-tree"))?;
    Ok(sha.trim().to_string())
}

/// A unique scratch-worktree path outside every worktree, keyed by the
/// compactor id and a nanosecond stamp (the same shape as the transfer's
/// patch path, §2.6).
fn scratch_worktree(compactor_id: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("litany-compaction-base-{compactor_id}-{nanos}"))
}
