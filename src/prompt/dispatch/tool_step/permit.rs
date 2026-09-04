//! The grant gate: what a role may **call** (ARCH §3.3 *declaring is not
//! permitting*, §4.3 *Toolset*).
//!
//! Split from [`super`] so the tool window's body stays under the repo's
//! 300-line code-file cap. It is one pure function over three facts —
//! the role, its `providers.yaml` grant, and everything injected into
//! this request — and the composer reads the third from the same place
//! ([`crate::prompt::dispatch::tools::injected`]), which is what keeps
//! declaring and permitting from drifting apart.

use crate::prompt::tool::inject::InjectedTool;

/// Why `role` may not call `tool`, or `None` when it may (ARCH §3.3
/// *declaring is not permitting*, §4.3 *Toolset*).
///
/// A role's **effective toolset** is its `providers.yaml` `tools:` grant
/// plus `injected` — everything the composer declared that no config did
/// ([`super::tools::injected`]: the compactor's pair for that role, and
/// the binding's host-injected tools, §3.3). The gate and the composer
/// read that list from the same place, so a host cannot declare a tool
/// the model is then refused for calling. Its request declares
/// more than that and must: the array is closed over the history it
/// ships (§3.3), and a branch inherits its dispatcher's transcript by
/// fork (§2.3), so the tools that dispatcher used are named in the
/// history whether or not this role was granted them.
///
/// Permitting does not follow from declaring. If it did, a grant would
/// widen itself the moment a dispatcher used a tool the child was
/// denied — voiding exactly the boundaries a grant exists to draw: a
/// read-only observer on an outward surface (§4.3) forked from a
/// dispatcher that speaks there, or the compactor's deletion-only
/// guarantee (§2.7). So the decline is in-band: an `is_error`
/// `tool_result` naming the role's own toolset, which the model reads
/// and steps on from, and the executor is never entered.
pub(in crate::prompt::dispatch) fn refusal(
    role: &str,
    grant: &[String],
    injected: &[InjectedTool],
    tool: &str,
) -> Option<String> {
    let injected: Vec<&str> = injected.iter().map(|t| t.name.as_str()).collect();
    if grant.iter().any(|granted| granted == tool) || injected.contains(&tool) {
        return None;
    }
    let mut effective: Vec<&str> = grant.iter().map(String::as_str).collect();
    effective.extend(injected);
    let toolset = if effective.is_empty() {
        "empty".to_string()
    } else {
        effective.join(", ")
    };
    Some(format!(
        "{tool:?} is not callable by a {role}: it is declared only because \
         the inherited transcript references it. The {role} toolset is \
         {toolset} (ARCH §3.3, declaring is not permitting)."
    ))
}
