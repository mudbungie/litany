//! The crate's module graph — reachability as the ground truth.
//!
//! Publicity is a property of a *path*, not of an item: a `pub fn` in a
//! private module is unreachable, and a `pub fn` anywhere down a chain of
//! `pub mod`s is public API wherever in the tree it lives. So the checker
//! starts at the crate roots (`src/lib.rs`, and the exec binding's
//! `src/bin/litany/main.rs`), walks every `mod` declaration — file-based
//! and inline alike — and carries two bits down each edge: whether the
//! whole chain is `pub` (externally reachable), and whether any ancestor
//! is `#[cfg(test)]` (compiled only under test, hence never public).
//!
//! [`source_files`] closes the walk: every `src/**/*.rs` on disk must be
//! met by it ([`surface::every_source_file_is_reachable`]), so no public
//! declaration can hide in a file the checker never opened.

use crate::items::is_pub;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use syn::Item;

/// One module of the graph: its `::` path, the file it was parsed from,
/// its own items, and the two inherited bits.
pub struct Module {
    /// `crate`, `crate::cmd`, `crate::cmd::new`, …
    pub path: String,
    /// The source file the module's items were parsed from. An inline
    /// module shares its parent's file.
    pub file: PathBuf,
    /// The module's own items (an inline child's items are its own).
    pub items: Vec<Item>,
    /// Externally reachable: every ancestor `mod` on the path is `pub`.
    pub public: bool,
    /// Under a `#[cfg(test)]` ancestor — compiled only for tests.
    pub test_only: bool,
}

/// The crate root directory (`CARGO_MANIFEST_DIR`).
pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

thread_local! {
    /// The walk is a pure function of the sources, so it is done once and
    /// shared: every assertion below reads the same graph.
    static CRATE_GRAPH: Rc<Vec<Module>> =
        Rc::new(walk(root().join("src/lib.rs"), "crate", true));
}

/// The library's module graph, rooted at `src/lib.rs` — the crate root is
/// public by definition, so publicity flows from its `pub mod`s.
pub fn crate_graph() -> Rc<Vec<Module>> {
    CRATE_GRAPH.with(Rc::clone)
}

/// The exec binding's module graph. A binary crate exports nothing, so
/// every module in it is non-public; it is walked only so that
/// [`source_files`] reachability is total over `src/`.
pub fn binding_graph() -> Vec<Module> {
    walk(root().join("src/bin/litany/main.rs"), "bin", false)
}

/// The one module at `path` (panics if the graph has no such module).
pub fn module<'a>(graph: &'a [Module], path: &str) -> &'a Module {
    graph
        .iter()
        .find(|m| m.path == path)
        .unwrap_or_else(|| panic!("module {path} is not in the crate's module graph"))
}

/// Every `.rs` file under `src/`, the denominator of the reachability
/// assertion.
pub fn source_files() -> BTreeSet<PathBuf> {
    let mut out = BTreeSet::new();
    collect_rs(&root().join("src"), &mut out);
    out
}

fn collect_rs(dir: &Path, out: &mut BTreeSet<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display())) {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_rs(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.insert(path);
        }
    }
}

/// Parse one source file into its items.
fn parse(file: &Path) -> Vec<Item> {
    let src =
        std::fs::read_to_string(file).unwrap_or_else(|e| panic!("read {}: {e}", file.display()));
    syn::parse_file(&src)
        .unwrap_or_else(|e| panic!("parse {}: {e}", file.display()))
        .items
}

/// Is this `mod` gated to test builds? Any `cfg` mentioning `test`
/// counts — the gate need not be a bare `#[cfg(test)]`.
fn is_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("cfg")
            && a.meta
                .require_list()
                .is_ok_and(|l| l.tokens.to_string().contains("test"))
    })
}

/// Where a module's file-based children live: beside a `mod.rs`/`lib.rs`/
/// `main.rs`, in a same-named subdirectory beside any other file.
fn child_dir(file: &Path) -> PathBuf {
    let parent = file
        .parent()
        .expect("source file has a parent")
        .to_path_buf();
    let stem = file
        .file_stem()
        .expect("source file has a stem")
        .to_string_lossy()
        .to_string();
    match stem.as_str() {
        "mod" | "lib" | "main" => parent,
        _ => parent.join(stem),
    }
}

/// `mod name;` → `<dir>/name.rs`, else `<dir>/name/mod.rs`.
fn resolve(dir: &Path, name: &str) -> PathBuf {
    let flat = dir.join(format!("{name}.rs"));
    if flat.is_file() {
        flat
    } else {
        dir.join(name).join("mod.rs")
    }
}

/// A module still to be visited: a [`Module`] plus the directory its
/// file-based children resolve against.
struct Pending {
    module: Module,
    dir: PathBuf,
}

/// Walk one crate root into its full module graph.
fn walk(file: PathBuf, path: &str, public: bool) -> Vec<Module> {
    let dir = child_dir(&file);
    let mut queue = vec![Pending {
        module: Module {
            path: path.to_string(),
            items: parse(&file),
            file,
            public,
            test_only: false,
        },
        dir,
    }];
    let mut out = Vec::new();
    while let Some(pending) = queue.pop() {
        let parent = &pending.module;
        for item in &parent.items {
            let Item::Mod(decl) = item else { continue };
            let name = decl.ident.to_string();
            let module = Module {
                path: format!("{}::{name}", parent.path),
                file: parent.file.clone(),
                items: Vec::new(),
                public: parent.public && is_pub(&decl.vis),
                test_only: parent.test_only || is_cfg_test(&decl.attrs),
            };
            let dir = pending.dir.join(&name);
            queue.push(match &decl.content {
                // An inline module: its items are here, in this file.
                Some((_, items)) => Pending {
                    module: Module {
                        items: items.clone(),
                        ..module
                    },
                    dir,
                },
                // A file module: parse the file it names.
                None => {
                    let file = resolve(&pending.dir, &name);
                    Pending {
                        module: Module {
                            items: parse(&file),
                            file,
                            ..module
                        },
                        dir,
                    }
                }
            });
        }
        out.push(pending.module);
    }
    out
}
