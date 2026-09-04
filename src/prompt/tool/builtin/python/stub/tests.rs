//! Unit tests for the stub generator. The shipped definitions are the
//! subject: what a program can call, and how, is read out of the very
//! bytes the model is told about (ARCH §3.3), so the module is pinned
//! against those rather than against a hand-written schema.

use super::*;
use serde_json::json;

/// A [`ToolDef`] built from the **shipped** schema and skill for `name`
/// — the same two files `dispatch::tools::compose` reads for the wire.
fn shipped(name: &str) -> ToolDef {
    let schema = crate::install::TOOLS
        .get_file(format!("{name}.json"))
        .expect("the tool pool ships this schema")
        .contents();
    let skill = crate::install::SKILLS
        .get_file(format!("{name}/SKILL.md"))
        .expect("the skill pool ships this skill")
        .contents_utf8()
        .expect("SKILL.md is UTF-8");
    let front = crate::skill::frontmatter_yaml(skill).expect("SKILL.md has frontmatter");
    ToolDef {
        name: name.to_string(),
        description: Some(
            crate::skill::parse(front)
                .expect("frontmatter parses")
                .description,
        ),
        input_schema: serde_json::from_slice(schema).expect("the shipped schema is JSON"),
    }
}

#[test]
fn a_function_is_the_shipped_schema_and_the_shipped_description() {
    let module = module(&[shipped("bash")], Path::new("/opt/litany"), "tu_1");
    assert!(
        module.contains("DOOR = [\"/opt/litany\", \"invoke\"]\n"),
        "{module}"
    );
    assert!(module.contains("TOOL_ID = \"tu_1\"\n"), "{module}");
    let expected = format!(
        "\n\ndef bash(*, command):\n    {doc}\n    return invoke(\"bash\", _arguments(command=command))\n",
        doc = literal(&shipped("bash").description.unwrap()),
    );
    assert!(module.ends_with(&expected), "{module}");
}

#[test]
fn a_required_parameter_has_no_default_and_an_optional_one_defaults_to_none() {
    // `search_history` takes exactly one of two, so neither is required
    // — the shipped shape that proves the `required` read is real.
    let module = module(
        &[shipped("search_history"), shipped("cd")],
        Path::new("l"),
        "t",
    );
    assert!(
        module.contains("\ndef search_history(*, entry=None, pattern=None):\n"),
        "{module}"
    );
    assert!(module.contains("\ndef cd(*, path):\n"), "{module}");
}

#[test]
fn required_parameters_come_first_and_the_rest_keep_the_schemas_order() {
    let tool = ToolDef {
        name: "fixture".to_string(),
        description: None,
        input_schema: json!({
            "properties": {"a": {}, "b": {}, "c": {}},
            "required": ["c"],
        }),
    };
    let module = module(&[tool], Path::new("l"), "t");
    assert!(
        module.contains("\ndef fixture(*, c, a=None, b=None):\n"),
        "{module}"
    );
    // No description committed: the docstring says so rather than being
    // absent, which would make the next line the docstring.
    assert!(
        module.contains("\"the fixture tool; no description is committed for it.\""),
        "{module}"
    );
}

#[test]
fn a_tool_whose_schema_takes_nothing_is_a_call_with_no_parameters() {
    let tool = ToolDef {
        name: "ping".to_string(),
        description: Some("no input".to_string()),
        input_schema: json!({"type": "object"}),
    };
    let module = module(&[tool], Path::new("l"), "t");
    assert!(
        module.contains(
            "\ndef ping():\n    \"no input\"\n    return invoke(\"ping\", _arguments())\n"
        ),
        "{module}"
    );
}

#[test]
fn a_name_python_cannot_spell_is_left_to_the_general_path() {
    // Neither the function nor the parameter is renamed here: `invoke`
    // reaches the tool, and the schema keeps the only spelling of its
    // own property name.
    let hosted = ToolDef {
        name: "server-tool".to_string(),
        description: Some("hosted".to_string()),
        input_schema: json!({}),
    };
    let odd = ToolDef {
        name: "odd".to_string(),
        description: Some("one spellable parameter".to_string()),
        input_schema: json!({"properties": {"ok": {}, "not-ok": {}}, "required": ["ok"]}),
    };
    assert!(!identifier("server-tool"));
    assert!(!identifier(""));
    assert!(identifier("_ok9"));
    let module = module(&[hosted, odd], Path::new("l"), "t");
    assert!(!module.contains("server-tool"), "{module}");
    assert!(module.contains("\ndef odd(*, ok):\n"), "{module}");
    assert!(!module.contains("not-ok"), "{module}");
}

#[test]
fn a_description_full_of_quotes_and_newlines_stays_one_python_literal() {
    let tool = ToolDef {
        name: "quoted".to_string(),
        description: Some("say \"hi\"\nthen \\ stop".to_string()),
        input_schema: json!({}),
    };
    let module = module(&[tool], Path::new("l"), "t");
    assert!(
        module.contains("    \"say \\\"hi\\\"\\nthen \\\\ stop\"\n"),
        "{module}"
    );
}
