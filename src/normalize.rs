use std::collections::BTreeMap;

use serde_json::Value;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::event::{CanonicalEvent, FieldValue};

fn parse_timestamp(raw: &str) -> Option<i64> {
    OffsetDateTime::parse(raw, &Rfc3339)
        .ok()
        .map(|t| (t.unix_timestamp_nanos() / 1_000_000) as i64)
}

fn json_to_field(v: &Value) -> Option<FieldValue> {
    match v {
        Value::String(s) => Some(FieldValue::Str(s.clone())),
        Value::Number(n) => n.as_f64().map(FieldValue::Num),
        Value::Bool(b) => Some(FieldValue::Bool(*b)),
        _ => None,
    }
}

/// Splits one raw transcript line into zero or more canonical events.
/// A line with multiple `content` blocks (e.g. an assistant turn with several
/// tool_use calls) yields one event per block, so each tool call is scored
/// independently against the corpus baseline.
pub fn normalize_line(source_file: &str, source_line: u64, raw: &str) -> Vec<CanonicalEvent> {
    let Ok(v) = serde_json::from_str::<Value>(raw) else {
        return Vec::new();
    };

    let record_type = v.get("type").and_then(Value::as_str).unwrap_or("");
    let session_id = v
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::to_string);
    let timestamp_raw = v
        .get("timestamp")
        .and_then(Value::as_str)
        .map(str::to_string);
    let timestamp_ms = timestamp_raw.as_deref().and_then(parse_timestamp);

    let base_fields = |extra: &Value| -> BTreeMap<String, FieldValue> {
        let mut fields = BTreeMap::new();
        if let Some(model) = v.pointer("/message/model").and_then(Value::as_str) {
            fields.insert("model".into(), FieldValue::Str(model.into()));
        }
        if let Some(cwd) = v.get("cwd").and_then(Value::as_str) {
            fields.insert("cwd".into(), FieldValue::Str(cwd.into()));
        }
        if let Value::Object(map) = extra {
            for (k, val) in map {
                if let Some(fv) = json_to_field(val) {
                    fields.insert(k.clone(), fv);
                }
            }
        }
        fields
    };

    let make = |action: Option<String>,
                status: Option<String>,
                duration_ms: Option<f64>,
                extra: &Value|
     -> CanonicalEvent {
        CanonicalEvent {
            source_file: source_file.to_string(),
            source_line,
            timestamp_ms,
            timestamp_raw: timestamp_raw.clone(),
            actor: session_id.clone(),
            action,
            status,
            duration_ms,
            fields: base_fields(extra),
        }
    };

    match record_type {
        "user" | "assistant" => {
            let role = v
                .pointer("/message/role")
                .and_then(Value::as_str)
                .unwrap_or(record_type);
            let content = v.pointer("/message/content");

            match content {
                Some(Value::Array(blocks)) => {
                    let mut events = Vec::with_capacity(blocks.len());
                    for block in blocks {
                        let block_type = block.get("type").and_then(Value::as_str).unwrap_or("");
                        match block_type {
                            "text" => {
                                events.push(make(
                                    Some(format!("{role}:text")),
                                    None,
                                    None,
                                    &Value::Null,
                                ));
                            }
                            "tool_use" => {
                                let name = block.get("name").and_then(Value::as_str);
                                let id = block.get("id").and_then(Value::as_str);
                                let extra = id.map_or(Value::Null, |id| {
                                    serde_json::json!({"tool_use_id": id})
                                });
                                events.push(make(name.map(str::to_string), None, None, &extra));
                            }
                            "tool_result" => {
                                let is_error = v
                                    .pointer("/toolUseResult/is_error")
                                    .or_else(|| block.get("is_error"))
                                    .and_then(Value::as_bool)
                                    .unwrap_or(false);
                                let status = if is_error { "error" } else { "ok" };
                                let id = block.get("tool_use_id").and_then(Value::as_str);
                                let extra = id.map_or(Value::Null, |id| {
                                    serde_json::json!({"tool_use_id": id})
                                });
                                events.push(make(
                                    Some("tool_result".to_string()),
                                    Some(status.to_string()),
                                    None,
                                    &extra,
                                ));
                            }
                            _ => {}
                        }
                    }
                    events
                }
                Some(Value::String(_)) => {
                    vec![make(Some(format!("{role}:text")), None, None, &Value::Null)]
                }
                _ => vec![make(Some(role.to_string()), None, None, &Value::Null)],
            }
        }
        "attachment" => {
            let atype = v
                .pointer("/attachment/type")
                .and_then(Value::as_str)
                .map(str::to_string);
            vec![make(
                Some(format!("attachment:{}", atype.unwrap_or_default())),
                None,
                None,
                &Value::Null,
            )]
        }
        "mode" | "permission-mode" => {
            vec![make(Some(record_type.to_string()), None, None, &v)]
        }
        _ => Vec::new(),
    }
}
