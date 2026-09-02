use std::fs;
use std::path::Path;

use serde_json::{json, Value};
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

const KAGI_EXPORT_MAX_BYTES: u64 = 16 * 1024 * 1024;

fn text(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn is_http_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

fn graph_from_kagi(bytes: &[u8]) -> Result<(String, Value), String> {
    let input = std::str::from_utf8(bytes).map_err(|_| "Invalid Kagi export JSON".to_string())?;
    let export: Value =
        serde_json::from_str(input).map_err(|_| "Invalid Kagi export JSON".to_string())?;
    let version = export.get("version").and_then(Value::as_i64);
    if version != Some(1) {
        return Err(format!(
            "Unsupported Kagi export version: {}",
            version.map_or_else(|| "unknown".to_string(), |v| v.to_string())
        ));
    }

    let conversation = export.get("conversation").and_then(Value::as_object);
    let title = conversation
        .and_then(|value| value.get("title"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or("Kagi conversation")
        .to_string();
    let messages = conversation
        .and_then(|value| value.get("messages"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut layout = Vec::new();
    let mut pending_user: Option<Value> = None;
    let mut previous_assistant: Option<String> = None;
    let mut turn_index = 0usize;

    let flush = |user: Value,
                 assistant: Option<Value>,
                 turn_index: usize,
                 nodes: &mut Vec<Value>,
                 edges: &mut Vec<Value>,
                 layout: &mut Vec<Value>,
                 previous_assistant: &mut Option<String>| {
        let user_id = format!("import:{}:turn:{}:user", urlencoding(&title), turn_index);
        let assistant_id = format!(
            "import:{}:turn:{}:assistant",
            urlencoding(&title),
            turn_index
        );
        nodes.push(json!({"id": user_id, "role": "user", "content": text(user.get("content")), "timestamp": turn_index * 2}));
        layout.push(json!({"id": user_id, "position": {"x": 0, "y": turn_index * 240}}));
        if let Some(previous) = previous_assistant.as_ref() {
            edges.push(json!({"id": format!("{}->{}", previous, user_id), "source": previous, "target": user_id}));
        }
        let mut assistant_node = json!({"id": assistant_id, "role": "assistant", "content": "", "timestamp": turn_index * 2 + 1});
        if let Some(assistant) = assistant {
            assistant_node["content"] = json!(text(assistant.get("content")));
            if let Some(model) = assistant.get("model_name").and_then(Value::as_str) {
                assistant_node["model"] = json!(model);
            }
            let cited: std::collections::HashSet<i64> = text(assistant.get("content"))
                .match_indices('【')
                .filter_map(|(start, _)| {
                    text(assistant.get("content"))[start + 3..]
                        .split('】')
                        .next()?
                        .parse()
                        .ok()
                })
                .collect();
            let references = assistant.get("references").and_then(Value::as_array).map(|refs| refs.iter().filter_map(|reference| {
                let object = reference.as_object()?;
                let url = object.get("url")?.as_str()?;
                if !is_http_url(url) { return None; }
                let index = object.get("index").and_then(Value::as_i64);
                let mut result = json!({"type": "url", "url": url, "relations": [if index.is_some_and(|value| cited.contains(&value)) { "cited" } else { "consulted" }]});
                for key in ["title", "domain"] {
                    if let Some(value) = object.get(key).and_then(Value::as_str) { result[key] = json!(value); }
                }
                for key in ["index", "percentage"] {
                    if let Some(value) = object.get(key).and_then(Value::as_f64) { result[key] = json!(value); }
                }
                if let Some(value) = object.get("is_search_result").and_then(Value::as_bool) { result["is_search_result"] = json!(value); }
                Some(result)
            }).collect::<Vec<_>>()).unwrap_or_default();
            assistant_node["provenance"] =
                json!({"completeness": "complete", "references": references, "activity": []});
        } else {
            assistant_node["incomplete"] = json!(true);
        }
        nodes.push(assistant_node);
        layout.push(json!({"id": assistant_id, "position": {"x": 0, "y": turn_index * 240 + 120}}));
        edges.push(json!({"id": format!("{}->{}", user_id, assistant_id), "source": user_id, "target": assistant_id}));
        *previous_assistant = Some(assistant_id);
    };

    for message in messages {
        match message.get("role").and_then(Value::as_str) {
            Some("user") => {
                if let Some(user) = pending_user.take() {
                    flush(
                        user,
                        None,
                        turn_index,
                        &mut nodes,
                        &mut edges,
                        &mut layout,
                        &mut previous_assistant,
                    );
                    turn_index += 1;
                }
                pending_user = Some(message);
            }
            Some("assistant") => {
                if let Some(user) = pending_user.take() {
                    flush(
                        user,
                        Some(message),
                        turn_index,
                        &mut nodes,
                        &mut edges,
                        &mut layout,
                        &mut previous_assistant,
                    );
                    turn_index += 1;
                }
            }
            _ => {
                if let Some(user) = pending_user.take() {
                    flush(
                        user,
                        None,
                        turn_index,
                        &mut nodes,
                        &mut edges,
                        &mut layout,
                        &mut previous_assistant,
                    );
                    turn_index += 1;
                }
            }
        }
    }
    if let Some(user) = pending_user {
        flush(
            user,
            None,
            turn_index,
            &mut nodes,
            &mut edges,
            &mut layout,
            &mut previous_assistant,
        );
    }

    Ok((
        title,
        json!({"version": 4, "nodes": nodes, "edges": edges, "layout": layout}),
    ))
}

fn urlencoding(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{:02X}", byte),
        })
        .collect()
}

pub(crate) fn import_kagi_export_from_path(path: &Path) -> Result<Value, String> {
    let metadata =
        fs::metadata(path).map_err(|error| format!("Failed to read Kagi export: {error}"))?;
    if metadata.len() > KAGI_EXPORT_MAX_BYTES {
        return Err(format!(
            "Kagi export exceeds the {KAGI_EXPORT_MAX_BYTES}-byte input limit ({} bytes)",
            metadata.len()
        ));
    }
    let bytes = fs::read(path).map_err(|error| format!("Failed to read Kagi export: {error}"))?;
    if bytes.len() as u64 > KAGI_EXPORT_MAX_BYTES {
        return Err(format!(
            "Kagi export exceeds the {KAGI_EXPORT_MAX_BYTES}-byte input limit ({} bytes)",
            bytes.len()
        ));
    }
    let (title, graph) = graph_from_kagi(&bytes)?;
    Ok(json!({"title": title, "graph": graph}))
}

#[tauri::command]
pub(crate) async fn import_kagi_export(path: String) -> Result<Value, String> {
    import_kagi_export_from_path(Path::new(&path))
}

#[tauri::command]
pub(crate) async fn pick_kagi_export(app: AppHandle) -> Result<Option<String>, String> {
    Ok(app
        .dialog()
        .file()
        .set_title("Import Kagi Export")
        .add_filter("JSON", &["json"])
        .blocking_pick_file()
        .map(|path| path.to_string()))
}

#[cfg(test)]
mod tests {
    use super::import_kagi_export_from_path;
    use std::path::Path;

    #[test]
    fn imports_sanitized_fixture_into_expected_graph() {
        let result = import_kagi_export_from_path(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../..//test/fixtures/kagi-export-v1.json"
        )))
        .unwrap();
        assert_eq!(result["title"], "Example research conversation");
        assert_eq!(result["graph"]["nodes"].as_array().unwrap().len(), 4);
    }
}
