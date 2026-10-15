use std::collections::BTreeMap;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum FieldValue {
    Str(String),
    Num(f64),
    Bool(bool),
}

#[derive(Debug, Clone, Serialize)]
pub struct CanonicalEvent {
    pub source_file: String,
    pub source_line: u64,
    pub timestamp_ms: Option<i64>,
    pub timestamp_raw: Option<String>,
    pub actor: Option<String>,
    pub action: Option<String>,
    pub status: Option<String>,
    pub duration_ms: Option<f64>,
    pub fields: BTreeMap<String, FieldValue>,
}
