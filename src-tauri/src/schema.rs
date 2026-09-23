//! Schema discovery helpers. HelixDB has no catalog, so the backend samples
//! labels and properties and returns a ready-to-render schema model.

use std::collections::{BTreeMap, HashSet};

use serde::Serialize;
use serde_json::Value;

use crate::{
    hql::is_keyword_word,
    results::{LabelCount, Row},
};

const MAX_FIELD_VALUES: usize = 3;

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Schema {
    pub node_labels: Vec<SchemaLabel>,
    pub edge_labels: Vec<SchemaLabel>,
    pub sample: u64,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SchemaField {
    pub name: String,
    pub types: Vec<String>,
    pub values: Vec<Value>,
    pub present_on: usize,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SchemaLabel {
    pub label: String,
    pub count: u64,
    pub browse_query: String,
    pub graph_query: String,
    pub fields: Vec<SchemaField>,
    pub field_sample: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field_error: Option<String>,
}

impl SchemaLabel {
    pub fn new(value: LabelCount, entity: &str) -> Self {
        let quoted = quote_identifier(&value.label);
        Self {
            label: value.label,
            count: value.count,
            browse_query: format!(
                "SELECT * FROM {}:{quoted} LIMIT 50",
                if entity == "nodes" { "NODES" } else { "EDGES" }
            ),
            graph_query: if entity == "nodes" {
                format!("QUERY NODES:{quoted} LIMIT 300")
            } else {
                format!("QUERY VIA {quoted} LIMIT 300")
            },
            fields: Vec::new(),
            field_sample: 0,
            field_error: None,
        }
    }
}

#[derive(Default)]
struct FieldBuilder {
    types: Vec<String>,
    values: Vec<Value>,
    value_keys: HashSet<String>,
    present_on: usize,
}

pub fn infer_schema_fields(rows: &[Row]) -> Vec<SchemaField> {
    let mut fields: BTreeMap<String, FieldBuilder> = BTreeMap::new();
    for row in rows {
        for (name, value) in row {
            if matches!(name.as_str(), "id" | "label" | "source" | "target") {
                continue;
            }
            let field = fields.entry(name.clone()).or_default();
            field.present_on += 1;
            let value_type = schema_value_type(value).to_owned();
            if !field.types.contains(&value_type) {
                field.types.push(value_type.clone());
            }
            let value_key = format!("{value_type}:{}", value_text(value));
            if field.values.len() < MAX_FIELD_VALUES && field.value_keys.insert(value_key) {
                field.values.push(value.clone());
            }
        }
    }
    fields
        .into_iter()
        .map(|(name, field)| SchemaField {
            name,
            types: field.types,
            values: field.values,
            present_on: field.present_on,
        })
        .collect()
}

pub fn schema_query_for(entity: &str, label: &str, limit: u64) -> String {
    let quoted = quote_identifier(label);
    format!(
        "SELECT * FROM {}:{quoted} LIMIT {}",
        if entity == "nodes" { "NODES" } else { "EDGES" },
        limit.max(1)
    )
}

fn quote_identifier(value: &str) -> String {
    if is_simple_identifier(value) && !is_keyword_word(&value.to_ascii_uppercase()) {
        value.to_owned()
    } else {
        format!("\"{}\"", value.replace('"', "\"\""))
    }
}

fn is_simple_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn schema_value_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(number) if number.is_i64() || number.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn value_text(value: &Value) -> String {
    match value {
        Value::Null => "null".into(),
        Value::String(value) if value.is_empty() => "\"\"".into(),
        Value::String(value) => value.clone(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        _ => serde_json::to_string(value).unwrap_or_else(|_| value.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Map};

    use super::*;

    #[test]
    fn infers_fields_and_excludes_entity_identity() {
        let rows = vec![
            Map::from_iter([
                ("id".into(), json!(1)),
                ("name".into(), json!("Alice")),
                ("age".into(), json!(30)),
            ]),
            Map::from_iter([
                ("id".into(), json!(2)),
                ("name".into(), json!("Bob")),
                ("age".into(), Value::Null),
            ]),
        ];
        let fields = infer_schema_fields(&rows);
        assert_eq!(
            fields
                .iter()
                .map(|field| field.name.as_str())
                .collect::<Vec<_>>(),
            ["age", "name"]
        );
        assert_eq!(fields[0].types, ["integer", "null"]);
        assert_eq!(fields[1].present_on, 2);
    }

    #[test]
    fn quotes_unusual_labels() {
        assert_eq!(
            schema_query_for("nodes", "User", 25),
            "SELECT * FROM NODES:User LIMIT 25"
        );
        assert_eq!(
            schema_query_for("edges", "Works \"at\"", 25),
            "SELECT * FROM EDGES:\"Works \"\"at\"\"\" LIMIT 25"
        );
        assert_eq!(
            schema_query_for("nodes", "Limit", 25),
            "SELECT * FROM NODES:\"Limit\" LIMIT 25"
        );
    }
}
