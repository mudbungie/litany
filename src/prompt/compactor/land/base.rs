//! The **compaction base** (ARCH §2.6): one commit whose tree is the
//! compaction point's with the product applied, parented on the span's
//! lower bound — the squash the landing rebases the live tail onto.
//!
//! What reaches it — nominations, the summary, and the transcript
//! entries of the span itself — is classified apart, in [`super::product`].
//!
//! The base is minted without disturbing the live checkout: a throwaway
//! `--no-checkout` worktree gives us a private index (no `GIT_INDEX_FILE`
//! plumbing, no worktree churn) — `read-tree <P>`, drop the deletions,
//! stage the summary blobs from the compactor's tip, `write-tree`,
//! `commit-tree -p <bound>`. Pure object-store writes; the branch ref
//! does not move here (the replay moves it,
//! [`crate::prompt::rebase_forward`]).

use super::super::{Error, checkpoint};
use super::product::Product;
use super::span::Span;
use crate::template::GitRunner;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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
