//! The command reference on the website (docs/specs/site.md §4), rendered
//! from the MCP tool table and each command's parameter schema, and checked
//! against the page, so the two cannot drift. `COLDOPEN_BLESS=1 cargo test
//! -p coldopen-mcp --test reference` rewrites the page; CI refuses to.

use serde_json::Value;

const PAGE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/guide/src/reference/commands.md"
);

/// Text for people: one line, without the spec references the code's
/// comments carry for developers.
fn one_line(s: &str) -> String {
    let mut out = s.split_whitespace().collect::<Vec<_>>().join(" ");
    while let Some(i) = out.find(" (docs/specs/") {
        match out[i..].find(')') {
            Some(j) => out.replace_range(i..i + j + 1, ""),
            None => break,
        }
    }
    // A `<color>` in the text would read as an HTML tag.
    out.replace('|', "\\|").replace('<', "\\<")
}

fn literal(v: &Value) -> String {
    format!(
        "`{}`",
        v.as_str().map_or_else(|| v.to_string(), str::to_string)
    )
}

/// A parameter's type, in words.
fn kind(v: &Value) -> String {
    if let Some(e) = v.get("enum").and_then(Value::as_array) {
        let names: Vec<String> = e.iter().filter(|x| !x.is_null()).map(literal).collect();
        return format!("one of {}", names.join(", "));
    }
    if let Some(c) = v.get("const") {
        return literal(c);
    }
    for key in ["anyOf", "oneOf"] {
        if let Some(alts) = v.get(key).and_then(Value::as_array) {
            let mut kinds: Vec<String> = alts
                .iter()
                .filter(|a| a.get("type").and_then(Value::as_str) != Some("null"))
                .map(kind)
                .collect();
            kinds.dedup();
            return kinds.join(" or ");
        }
    }
    let types: Vec<&str> = match v.get("type") {
        Some(Value::String(t)) => vec![t.as_str()],
        Some(Value::Array(ts)) => ts
            .iter()
            .filter_map(Value::as_str)
            .filter(|t| *t != "null")
            .collect(),
        _ => Vec::new(),
    };
    let words: Vec<String> = types
        .iter()
        .map(|t| match *t {
            "array" => match v.get("items") {
                Some(items) => format!("list of {}", kind(items)),
                None => "list".into(),
            },
            "integer" => "whole number".into(),
            "boolean" => "true or false".into(),
            other => other.to_string(),
        })
        .collect();
    if words.is_empty() {
        "any".into()
    } else {
        words.join(" or ")
    }
}

const INTRO: &str = r#"# Command reference

<!-- Made from the MCP tool table and the commands' parameter types by crates/mcp/tests/reference.rs. Don't edit this page: change the code, then run `COLDOPEN_BLESS=1 cargo test -p coldopen-mcp --test reference`. -->

Everything Cold Open does to a script is one of these commands. The same names and parameters work everywhere:

- in a JSON batch for `coldopen run`, and over `coldopen serve --stdio` (`{"v": 1, "id": 1, "command": "block.insert", "params": {…}}`);
- as tools of the MCP server `coldopen mcp`, where the dot becomes an underscore (`block.insert` is the tool `block_insert`);
- in the app's palette, which lists every one.

Blocks are named by their ids (`b_42`), which stay the same as the script changes. Positions in a block's text count characters. The full rules, with every error code, are in the [command spec](https://github.com/TheYungStump/cold-open/blob/main/docs/specs/commands.md).

"#;

fn render() -> String {
    let mut out = String::from(INTRO);
    let tools = coldopen_mcp::tools::tools();
    for (command, description) in coldopen_mcp::tools::TOOLS {
        let name = coldopen_mcp::tools::tool_name(command);
        let Some(tool) = tools.iter().find(|t| t.name == name) else {
            continue;
        };
        out.push_str(&format!(
            "## `{command}`\n\nMCP tool `{name}`. {}\n\n",
            one_line(description)
        ));
        let schema = &*tool.input_schema;
        let required: Vec<&str> = schema
            .get("required")
            .and_then(Value::as_array)
            .map(|r| r.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        match schema.get("properties").and_then(Value::as_object) {
            Some(props) if !props.is_empty() => {
                out.push_str("| Parameter | Type | | What it is |\n| --- | --- | --- | --- |\n");
                for (p, v) in props {
                    let need = if required.contains(&p.as_str()) {
                        "required"
                    } else {
                        "optional"
                    };
                    let what = v
                        .get("description")
                        .and_then(Value::as_str)
                        .map(one_line)
                        .unwrap_or_default();
                    out.push_str(&format!("| `{p}` | {} | {need} | {what} |\n", kind(v)));
                }
                out.push('\n');
            }
            _ => out.push_str("No parameters.\n\n"),
        }
    }
    out.trim_end().to_string() + "\n"
}

#[test]
fn the_command_reference_matches_the_code() {
    let want = render();
    if std::env::var("COLDOPEN_BLESS").is_ok_and(|v| v == "1") {
        assert!(
            !std::env::var("CI").is_ok_and(|v| !v.is_empty()),
            "refusing to bless the command reference with CI set"
        );
        std::fs::write(PAGE, &want).unwrap();
        return;
    }
    let have = std::fs::read_to_string(PAGE)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert!(
        have == want,
        "docs/guide/src/reference/commands.md is out of date: run `COLDOPEN_BLESS=1 cargo test -p coldopen-mcp --test reference` and review it"
    );
}
