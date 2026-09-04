//! What the **binding** injects into a built-in (ARCH §2.11, §3.3
//! *Host-injected tools*) — one value rather than four parameters on
//! every arm of [`super::run`].

use std::path::Path;

/// Everything a built-in may need from the binding: the re-entry path
/// the `dispatch` / `message` built-ins and a program's stub module go
/// back through the front door with, the
/// adapter target and stop flag a caller resolution runs under, and the
/// host's tool injection. `bash` and the rest read none of it; `python`
/// reads all four, because a program's toolset is resolved exactly where
/// the door resolves it (`docs/DESIGN_CODE_EXECUTION.md` §2.7).
pub struct Bindings<'a> {
    /// `cmd::Fx::driver_target` (§2.11) — never a `litany` resolved by
    /// name.
    pub driver_target: &'a Path,
    pub adapter_target: Option<&'a Path>,
    pub stop: &'a std::sync::atomic::AtomicBool,
    pub injection: Option<&'a dyn crate::prompt::tool::inject::ToolInjection>,
}
