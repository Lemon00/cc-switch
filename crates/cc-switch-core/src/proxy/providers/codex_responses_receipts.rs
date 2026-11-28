//! Preserve standalone tool receipts for native third-party Responses upstreams.

use serde_json::{json, Value};

const ORPHAN_TOOL_OUTPUT_MARKER: &str = "[tool_result without a matching tool_call]";
const MESSAGE_CONTENT_PART_TYPES: &[&str] = &["input_text", "input_image", "input_file"];

pub fn rewrite_call_id_less_tool_outputs(body: &mut Value) -> bool {
    let Some(input) = body.get_mut("input").and_then(Value::as_array_mut) else {
        return false;
    };
    let mut changed = false;
    for item in input {
        if json_type(item) != Some("function_call_output") {
            continue;
        }
        let has_call_id = item
            .get("call_id")
            .and_then(Value::as_str)
            .is_some_and(|id| !id.trim().is_empty());
        if has_call_id {
            continue;
        }
        *item = json!({
            "type": "message",
            "role": "user",
            "content": orphan_tool_output_content(item.get("output")),
        });
        changed = true;
    }
    changed
}

fn orphan_tool_output_content(output: Option<&Value>) -> Vec<Value> {
    match output {
        Some(Value::String(text)) if !text.is_empty() => {
            vec![input_text_part(&format!(
                "{ORPHAN_TOOL_OUTPUT_MARKER}\n{text}"
            ))]
        }
        Some(Value::Array(parts)) => {
            let mut content = vec![input_text_part(ORPHAN_TOOL_OUTPUT_MARKER)];
            content.extend(parts.iter().map(|part| {
                if json_type(part).is_some_and(|ty| MESSAGE_CONTENT_PART_TYPES.contains(&ty)) {
                    part.clone()
                } else {
                    input_text_part(&part.to_string())
                }
            }));
            content
        }
        Some(Value::Null | Value::String(_)) | None => {
            vec![input_text_part(ORPHAN_TOOL_OUTPUT_MARKER)]
        }
        Some(other) => vec![input_text_part(&format!(
            "{ORPHAN_TOOL_OUTPUT_MARKER}\n{other}"
        ))],
    }
}

fn json_type(value: &Value) -> Option<&str> {
    value.get("type").and_then(Value::as_str).map(str::trim)
}

fn input_text_part(text: &str) -> Value {
    json!({ "type": "input_text", "text": text })
}
