//! The **stub module** generator (`docs/DESIGN_CODE_EXECUTION.md` §2.7).
//!
//! `litany_tools` is a python module written fresh for every `python`
//! invocation from the calling role's effective toolset: one
//! keyword-only function per tool, its parameters read from the tool's
//! committed schema, its docstring the tool's own description, each of
//! them one `subprocess.run` of the door verb ([`super::super::super`]'s
//! `litany invoke`, ARCH §3.4). Nothing here decides anything: what may
//! be called is the door's, and this module only offers what the door
//! would permit at the moment it was generated.
//!
//! Two facts are baked in, both the built-in's own: the **driver
//! target** (the binding-injected re-entry path, ARCH §2.11 — never a
//! `litany` resolved by name) and the enclosing invocation's
//! `tool_use.id`, from which the stub mints each inner invocation's
//! derived id `<tool-id>-<k>` in program order (§2.3). So the program's
//! environment carries nothing new and no path is written into the
//! worktree.
//!
//! `python` is absent from its own module — depth 1 (ARCH §3.3), the
//! multi-tool's rule for the multi-tool's reason — and so is any name
//! that is not a python identifier, which `invoke(name, arguments)`
//! reaches instead: the general path the generated functions are sugar
//! over.

use serde_json::Value;
use std::path::Path;

/// One tool the program may call: the three facts a `tools: [...]`
/// entry carries (ARCH §3.3), read where the composer reads them.
pub(crate) struct ToolDef {
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    pub(crate) input_schema: Value,
}

/// The whole `litany_tools` module for `tools`, minted against `door`
/// (the driver target) and `tool_id` (the enclosing invocation's id).
pub(crate) fn module(tools: &[ToolDef], door: &Path, tool_id: &str) -> String {
    let mut out = String::from(PREAMBLE);
    out.push_str(&format!(
        "DOOR = [{}, \"invoke\"]\nTOOL_ID = {}\n{}",
        literal(&door.to_string_lossy()),
        literal(tool_id),
        RUNTIME
    ));
    // A name python cannot spell gets no function; `invoke(name, …)`
    // reaches it, and renaming it here would be a second spelling of a
    // name the toolset already owns.
    for tool in tools.iter().filter(|t| identifier(&t.name)) {
        out.push_str(&function(tool));
    }
    out
}

/// One generated function: `def <name>(*, <params>)`, its docstring, and
/// the one call into [`RUNTIME`]'s `invoke`.
fn function(tool: &ToolDef) -> String {
    let params = parameters(&tool.input_schema);
    let named = params
        .iter()
        .map(|(name, required)| {
            if *required {
                name.clone()
            } else {
                format!("{name}=None")
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    // A tool whose schema takes nothing gets `def name():` — `*,` with
    // no parameter after it is a syntax error, and the empty signature
    // is the general path with empty inputs, not a special case.
    let signature = if named.is_empty() {
        String::new()
    } else {
        format!("*, {named}")
    };
    let arguments = params
        .iter()
        .map(|(name, _)| format!("{name}={name}"))
        .collect::<Vec<_>>()
        .join(", ");
    let doc = tool.description.clone().unwrap_or_else(|| {
        format!(
            "the {} tool; no description is committed for it.",
            tool.name
        )
    });
    format!(
        "\n\ndef {name}({signature}):\n    {doc}\n    return invoke({name_literal}, _arguments({arguments}))\n",
        name = tool.name,
        doc = literal(&doc),
        name_literal = literal(&tool.name),
    )
}

/// The schema's own parameters: every `properties` key that is a python
/// identifier, each flagged by whether `required` names it. A property
/// name that is not an identifier is left to `invoke`, the general path,
/// rather than being renamed here — a renamed parameter would be a
/// second spelling of a name the schema already owns.
fn parameters(schema: &Value) -> Vec<(String, bool)> {
    let required: Vec<&str> = schema["required"]
        .as_array()
        .map(|names| names.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let Some(properties) = schema["properties"].as_object() else {
        return Vec::new();
    };
    let mut params: Vec<(String, bool)> = properties
        .keys()
        .filter(|name| identifier(name))
        .map(|name| (name.clone(), required.contains(&name.as_str())))
        .collect();
    // Required first, so the signature reads as the schema does; stable
    // within each half, so the module is a pure function of the schemas.
    params.sort_by_key(|(_, required)| !*required);
    params
}

/// Whether `name` can be spelled as a python identifier — ASCII only,
/// which every tool and property name litany ships is.
pub(crate) fn identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `text` as a python string literal. JSON's string escapes are a subset
/// of python's, so serialization is the escaping — a description full of
/// quotes, backslashes and newlines cannot break the module.
fn literal(text: &str) -> String {
    Value::String(text.to_string()).to_string()
}

/// The module's head: its own docstring and imports.
const PREAMBLE: &str = r#""""litany_tools — the tools you may call from this program.

Generated fresh for this one `python` invocation from the calling role's
effective toolset, so what this module offers is what the harness
permits (docs/DESIGN_CODE_EXECUTION.md §2.7). Each function runs ONE
inner invocation through litany's front door: gated, adjudicated and
recorded exactly as a top-level tool call would be, and entering the
model's context nowhere. Only what this program prints on stdout does,
so print what matters and let the rest stay here.
"""

import json
import subprocess

"#;

/// The module's body: everything the generated functions call.
const RUNTIME: &str = r#"
_counter = 0


class DoorError(RuntimeError):
    """The door could not be reached, so the invocation never ran.

    A tool that ran and failed is never this: it comes back as a Result
    with a non-zero exit_code, because a program filtering failures
    wants the code.
    """


class Result:
    """One inner invocation's result: the tool's raw result envelope,
    split into the streams that carry it."""

    def __init__(self, stdout, stderr, exit_code):
        self.stdout = stdout
        self.stderr = stderr
        self.exit_code = exit_code
        self.ok = exit_code == 0

    def __repr__(self):
        return "Result(exit_code=%r, ok=%r, stdout=%r, stderr=%r)" % (
            self.exit_code,
            self.ok,
            self.stdout,
            self.stderr,
        )


def invoke(name, arguments=None):
    """Run one inner invocation of `name` with `arguments`, and return
    its Result. This is the general path every generated function below
    is sugar over; call it directly for a tool this module does not
    name."""
    global _counter
    _counter += 1
    block = json.dumps(
        {
            "id": "%s-%d" % (TOOL_ID, _counter),
            "name": name,
            "input": arguments if arguments is not None else {},
        }
    )
    try:
        done = subprocess.run(DOOR, input=block, capture_output=True, text=True)
    except OSError as exc:
        raise DoorError("could not run %r: %s" % (DOOR, exc)) from exc
    return Result(done.stdout, done.stderr, done.returncode)


def _arguments(**kwargs):
    """The invocation's input object: every argument the caller named,
    with the ones it left unset omitted rather than sent as null."""
    return {key: value for key, value in kwargs.items() if value is not None}
"#;

#[cfg(test)]
mod tests;
