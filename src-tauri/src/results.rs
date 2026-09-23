//! HelixDB response decoding into stable, UI-facing view models.

use std::collections::{BTreeSet, HashMap, HashSet};

use serde::Serialize;
use serde_json::{Map, Number, Value};

use crate::hql::ResultShape;

const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

pub type Row = Map<String, Value>;

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GraphNodeData {
    pub id: String,
    pub label: Option<String>,
    pub properties: Row,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdgeData {
    pub id: String,
    pub label: Option<String>,
    pub source: String,
    pub target: String,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GraphData {
    pub nodes: Vec<GraphNodeData>,
    pub edges: Vec<GraphEdgeData>,
    pub dangling_edges: usize,
    pub truncated_nodes: bool,
    pub truncated_edges: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LabelCount {
    pub label: String,
    pub count: u64,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum QueryResult {
    Rows {
        columns: Vec<String>,
        rows: Vec<Row>,
    },
    Count {
        value: u64,
    },
    GroupCount {
        by: String,
        groups: Vec<LabelCount>,
    },
    Graph {
        graph: GraphData,
    },
    Labels {
        nodes: Option<Vec<LabelCount>>,
        edges: Option<Vec<LabelCount>>,
    },
    Stats {
        node_count: u64,
        edge_count: u64,
        node_labels: Vec<LabelCount>,
        edge_labels: Vec<LabelCount>,
    },
    Describe {
        #[serde(flatten)]
        data: Box<DescribeResult>,
    },
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DescribeResult {
    pub entity: &'static str,
    pub id: Option<String>,
    pub label: Option<String>,
    pub properties: Row,
    pub edges: Vec<GraphEdgeData>,
    pub neighbours: Vec<GraphNodeData>,
    pub degree: Option<u64>,
    pub endpoints: Option<Endpoints>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Endpoints {
    pub source: Option<GraphNodeData>,
    pub target: Option<GraphNodeData>,
}

#[derive(Clone, Debug, thiserror::Error)]
#[error("{message}")]
pub struct ResultError {
    pub message: String,
    pub detail: Option<String>,
}

impl ResultError {
    fn new(message: impl Into<String>, body: &Value) -> Self {
        Self {
            message: message.into(),
            detail: Some(preview(body)),
        }
    }
}

pub fn decode_response(body: &str, shape: &ResultShape) -> Result<QueryResult, ResultError> {
    let parsed = parse_body(body)?;
    read_result(&parsed, shape)
}

/// Decodes property rows without applying the JSON-safe integer conversion
/// used at the IPC boundary. Schema inference needs the original JSON number
/// type so an i64 outside JavaScript's safe range is still classified as an
/// integer rather than as a string.
pub fn decode_rows_for_schema(body: &str, shape: &ResultShape) -> Result<Vec<Row>, ResultError> {
    let parsed = parse_body(body)?;
    let ResultShape::Rows {
        variable,
        identity_variable,
        ..
    } = shape
    else {
        return Err(ResultError::new(
            "the schema sample did not compile to a row result",
            &parsed,
        ));
    };
    read_rows(&parsed, variable, identity_variable.as_deref())
}

fn parse_body(body: &str) -> Result<Value, ResultError> {
    serde_json::from_str(body).map_err(|error| ResultError {
        message: "the instance returned a body that is not JSON".into(),
        detail: Some(error.to_string()),
    })
}

pub fn read_result(body: &Value, shape: &ResultShape) -> Result<QueryResult, ResultError> {
    match shape {
        ResultShape::Rows {
            variable,
            columns,
            identity_variable,
        } => {
            let mut rows = read_rows(body, variable, identity_variable.as_deref())?;
            make_rows_safe(&mut rows);
            Ok(QueryResult::Rows {
                columns: columns.clone().unwrap_or_else(|| collect_columns(&rows)),
                rows,
            })
        }
        ResultShape::Count { variable } => Ok(QueryResult::Count {
            value: read_count(pick_variable(body, variable)?)?,
        }),
        ResultShape::GroupCount { variable, by } => Ok(QueryResult::GroupCount {
            by: by.clone(),
            groups: read_group_count(pick_variable(body, variable)?)?,
        }),
        ResultShape::Graph {
            node_variable,
            edge_variable,
            node_limit,
            edge_limit,
        } => Ok(QueryResult::Graph {
            graph: read_graph(body, node_variable, edge_variable, *node_limit, *edge_limit)?,
        }),
        ResultShape::Labels { nodes, edges } => Ok(QueryResult::Labels {
            nodes: nodes
                .as_ref()
                .map(|name| read_group_count(pick_variable(body, name)?))
                .transpose()?,
            edges: edges
                .as_ref()
                .map(|name| read_group_count(pick_variable(body, name)?))
                .transpose()?,
        }),
        ResultShape::Stats {
            node_count,
            edge_count,
            node_labels,
            edge_labels,
        } => Ok(QueryResult::Stats {
            node_count: read_count(pick_variable(body, node_count)?)?,
            edge_count: read_count(pick_variable(body, edge_count)?)?,
            node_labels: read_group_count(pick_variable(body, node_labels)?)?,
            edge_labels: read_group_count(pick_variable(body, edge_labels)?)?,
        }),
        ResultShape::DescribeNode => read_node_description(body),
        ResultShape::DescribeEdge => read_edge_description(body),
    }
}

fn read_rows(
    body: &Value,
    variable: &str,
    identity_variable: Option<&str>,
) -> Result<Vec<Row>, ResultError> {
    let mut rows = as_rows(pick_variable(body, variable)?, "rows")?
        .into_iter()
        .map(normalize_display_row)
        .collect::<Vec<_>>();
    if let Some(identity_variable) = identity_variable {
        let identity = as_rows(pick_variable(body, identity_variable)?, "identity")?;
        rows = merge_identity(rows, identity);
    }
    Ok(rows)
}

fn pick_variable<'a>(body: &'a Value, name: &str) -> Result<&'a Value, ResultError> {
    if body.is_array() || !body.is_object() {
        return Ok(body);
    }
    let object = body.as_object().expect("checked object");
    if let Some(value) = object.get(name) {
        return Ok(value);
    }
    for wrapper in ["data", "result", "results"] {
        if let Some(value) = object
            .get(wrapper)
            .and_then(Value::as_object)
            .and_then(|inner| inner.get(name))
        {
            return Ok(value);
        }
    }
    if object.len() == 1 {
        return Ok(object.values().next().expect("one value"));
    }
    Err(ResultError::new(
        format!("the response has no variable named {name:?}"),
        body,
    ))
}

fn read_count(value: &Value) -> Result<u64, ResultError> {
    let unwrapped = value
        .as_array()
        .and_then(|values| values.first())
        .unwrap_or(value);
    if let Some(value) = number_to_u64(unwrapped) {
        return Ok(value);
    }
    if let Some(object) = unwrapped.as_object() {
        for key in ["count", "value", "total"] {
            if let Some(value) = object.get(key).and_then(number_to_u64) {
                return Ok(value);
            }
        }
    }
    Err(ResultError::new("expected a count", value))
}

fn number_to_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|number| u64::try_from(number).ok()))
        .or_else(|| value.as_str().and_then(|number| number.parse().ok()))
}

fn read_group_count(value: &Value) -> Result<Vec<LabelCount>, ResultError> {
    let unwrapped = value
        .as_object()
        .and_then(|object| object.get("properties"))
        .filter(|value| value.is_array())
        .unwrap_or(value);
    let mut groups = Vec::new();
    if let Some(object) = unwrapped.as_object() {
        for (label, count) in object {
            groups.push(LabelCount {
                label: label.clone(),
                count: number_to_u64(count).unwrap_or(0),
            });
        }
    } else if let Some(entries) = unwrapped.as_array() {
        let mut sampled: HashMap<String, u64> = HashMap::new();
        for entry in entries.iter().filter_map(Value::as_object) {
            let label = ["key", "group", "label", "value", "_label", "$label"]
                .into_iter()
                .find_map(|key| entry.get(key));
            let Some(label) = label else { continue };
            let name = value_text(label);
            let count = ["count", "total", "n"]
                .into_iter()
                .find_map(|key| entry.get(key))
                .and_then(number_to_u64);
            if let Some(count) = count {
                groups.push(LabelCount { label: name, count });
            } else {
                *sampled.entry(name).or_default() += 1;
            }
        }
        groups.extend(
            sampled
                .into_iter()
                .map(|(label, count)| LabelCount { label, count }),
        );
    } else {
        return Err(ResultError::new("expected a grouped count", value));
    }
    groups.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.label.cmp(&right.label))
    });
    Ok(groups)
}

fn as_rows(value: &Value, context: &str) -> Result<Vec<Row>, ResultError> {
    if value.is_null() {
        return Ok(Vec::new());
    }
    if let Some(values) = value.as_array() {
        return Ok(values
            .iter()
            .filter_map(Value::as_object)
            .cloned()
            .collect());
    }
    if let Some(object) = value.as_object() {
        if let Some(properties) = object.get("properties").and_then(Value::as_array) {
            return Ok(properties
                .iter()
                .filter_map(Value::as_object)
                .cloned()
                .collect());
        }
        return Ok(vec![object.clone()]);
    }
    Err(ResultError::new(
        format!("expected rows for {context}"),
        value,
    ))
}

fn read_graph(
    body: &Value,
    node_variable: &str,
    edge_variable: &str,
    node_limit: u64,
    edge_limit: u64,
) -> Result<GraphData, ResultError> {
    let node_rows = as_rows(pick_variable(body, node_variable)?, "graph nodes")?;
    let edge_rows = as_rows(pick_variable(body, edge_variable)?, "graph edges")?;
    let mut nodes = Vec::new();
    let mut seen = HashSet::new();
    for row in &node_rows {
        let Some(mut node) = read_graph_node(row) else {
            continue;
        };
        if !seen.insert(node.id.clone()) {
            continue;
        }
        make_row_safe(&mut node.properties);
        nodes.push(node);
    }
    let mut edges = Vec::new();
    let mut edge_ids = HashSet::new();
    let mut dangling_edges = 0;
    for row in &edge_rows {
        let Some(edge) = read_graph_edge(row) else {
            continue;
        };
        if !edge_ids.insert(edge.id.clone()) {
            continue;
        }
        if !seen.contains(&edge.source) || !seen.contains(&edge.target) {
            dangling_edges += 1;
            continue;
        }
        edges.push(edge);
    }
    Ok(GraphData {
        nodes,
        edges,
        dangling_edges,
        truncated_nodes: node_rows.len() as u64 >= node_limit,
        truncated_edges: edge_rows.len() as u64 >= edge_limit,
    })
}

fn read_graph_node(row: &Row) -> Option<GraphNodeData> {
    let id = read_id(first(row, &["_id", "$id", "id"])?)?;
    let label = first(row, &["_label", "$label", "label"]).and_then(read_label);
    let mut properties = Row::new();
    for (key, value) in row {
        if !matches!(key.as_str(), "_id" | "_label" | "$id" | "$label") {
            properties.insert(key.clone(), value.clone());
        }
    }
    Some(GraphNodeData {
        id,
        label,
        properties,
    })
}

fn read_graph_edge(row: &Row) -> Option<GraphEdgeData> {
    Some(GraphEdgeData {
        id: read_id(first(row, &["_id", "$id", "id"])?)?,
        label: first(row, &["_label", "$label", "label"]).and_then(read_label),
        source: read_id(first(row, &["_src", "$from", "from", "source"])?)?,
        target: read_id(first(row, &["_dst", "$to", "to", "target"])?)?,
    })
}

fn first<'a>(row: &'a Row, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|key| row.get(*key))
}

fn read_id(value: &Value) -> Option<String> {
    match value {
        Value::String(value) if !value.is_empty() => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

fn read_label(value: &Value) -> Option<String> {
    value
        .as_str()
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn merge_identity(rows: Vec<Row>, identity: Vec<Row>) -> Vec<Row> {
    if rows.len() != identity.len() {
        return rows;
    }
    rows.into_iter()
        .zip(identity)
        .map(|(row, identity)| {
            let mut properties = row
                .into_iter()
                .filter(|(key, _)| !matches!(key.as_str(), "$id" | "$label" | "$from" | "$to"))
                .collect::<Row>();
            for (key, value) in identity {
                let alias = match key.as_str() {
                    "$id" => "id",
                    "$label" => "label",
                    "$from.$id" | "$from" => "source",
                    "$to.$id" | "$to" => "target",
                    _ => continue,
                };
                properties.insert(alias.into(), value);
            }
            properties
        })
        .collect()
}

fn normalize_display_row(row: Row) -> Row {
    let mut normalized = row
        .iter()
        .filter(|(key, _)| !matches!(key.as_str(), "$id" | "$label" | "$from" | "$to"))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<Row>();
    for (source, target) in [
        ("$id", "id"),
        ("$label", "label"),
        ("$from", "source"),
        ("$to", "target"),
    ] {
        if let Some(value) = row.get(source) {
            normalized.insert(target.into(), value.clone());
        }
    }
    normalized
}

fn collect_columns(rows: &[Row]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    for row in rows {
        seen.extend(row.keys().cloned());
    }
    let mut columns = ["id", "label", "source", "target"]
        .into_iter()
        .filter(|key| seen.remove(*key))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    columns.extend(seen);
    columns
}

fn read_node_description(body: &Value) -> Result<QueryResult, ResultError> {
    let mut properties = as_rows(pick_variable(body, "entity")?, "entity")?
        .into_iter()
        .next()
        .unwrap_or_default();
    properties.retain(|key, _| !matches!(key.as_str(), "$id" | "$label" | "$from" | "$to"));
    make_row_safe(&mut properties);
    let identity = as_rows(pick_variable(body, "identity")?, "identity")?
        .into_iter()
        .next()
        .unwrap_or_default();
    let edges = as_rows(pick_variable(body, "edges")?, "incident edges")?
        .iter()
        .filter_map(read_graph_edge)
        .collect();
    let mut neighbours = as_rows(pick_variable(body, "neighbours")?, "neighbours")?
        .iter()
        .filter_map(read_graph_node)
        .collect::<Vec<_>>();
    for neighbour in &mut neighbours {
        make_row_safe(&mut neighbour.properties);
    }
    Ok(QueryResult::Describe {
        data: Box::new(DescribeResult {
            entity: "nodes",
            id: identity.get("$id").and_then(read_id),
            label: identity.get("$label").and_then(read_label),
            properties,
            edges,
            neighbours,
            degree: Some(read_count(pick_variable(body, "degree")?)?),
            endpoints: None,
        }),
    })
}

fn read_edge_description(body: &Value) -> Result<QueryResult, ResultError> {
    let mut properties = as_rows(pick_variable(body, "entity")?, "entity")?
        .into_iter()
        .next()
        .unwrap_or_default();
    properties.retain(|key, _| !matches!(key.as_str(), "$id" | "$label" | "$from" | "$to"));
    make_row_safe(&mut properties);
    let identity = as_rows(pick_variable(body, "identity")?, "identity")?
        .into_iter()
        .next()
        .unwrap_or_default();
    let source = as_rows(pick_variable(body, "sourceNode")?, "source node")?
        .first()
        .and_then(read_graph_node);
    let target = as_rows(pick_variable(body, "targetNode")?, "target node")?
        .first()
        .and_then(read_graph_node);
    Ok(QueryResult::Describe {
        data: Box::new(DescribeResult {
            entity: "edges",
            id: identity.get("$id").and_then(read_id),
            label: identity.get("$label").and_then(read_label),
            properties,
            edges: Vec::new(),
            neighbours: Vec::new(),
            degree: None,
            endpoints: Some(Endpoints { source, target }),
        }),
    })
}

fn make_rows_safe(rows: &mut [Row]) {
    for row in rows {
        make_row_safe(row);
    }
}

fn make_row_safe(row: &mut Row) {
    for value in row.values_mut() {
        make_value_safe(value);
    }
}

fn make_value_safe(value: &mut Value) {
    match value {
        Value::Number(number) if number_outside_js_range(number) => {
            *value = Value::String(number.to_string());
        }
        Value::Array(values) => {
            for value in values {
                make_value_safe(value);
            }
        }
        Value::Object(values) => {
            for value in values.values_mut() {
                make_value_safe(value);
            }
        }
        _ => {}
    }
}

fn number_outside_js_range(number: &Number) -> bool {
    number
        .as_i64()
        .is_some_and(|value| !(-MAX_SAFE_INTEGER..=MAX_SAFE_INTEGER).contains(&value))
        || number
            .as_u64()
            .is_some_and(|value| value > MAX_SAFE_INTEGER as u64)
}

fn value_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        _ => serde_json::to_string(value).unwrap_or_else(|_| value.to_string()),
    }
}

fn preview(body: &Value) -> String {
    let text = serde_json::to_string(body).unwrap_or_else(|_| body.to_string());
    if text.chars().count() <= 600 {
        text
    } else {
        format!("{}…", text.chars().take(600).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn reads_rows_from_supported_envelopes() {
        let shape = ResultShape::Rows {
            variable: "rows".into(),
            columns: None,
            identity_variable: None,
        };
        for body in [
            json!({ "rows": [{ "$id": 1, "$label": "User", "name": "Alice" }] }),
            json!({ "data": { "rows": [{ "id": 1 }] } }),
            json!([{ "id": 1 }]),
            json!({ "rows": { "properties": [{ "$id": 1 }] } }),
        ] {
            let QueryResult::Rows { rows, .. } = read_result(&body, &shape).unwrap() else {
                panic!("expected rows");
            };
            assert_eq!(rows.len(), 1);
        }
    }

    #[test]
    fn graph_reader_deduplicates_and_drops_dangling_edges() {
        let shape = ResultShape::Graph {
            node_variable: "nodes".into(),
            edge_variable: "edges".into(),
            node_limit: 10,
            edge_limit: 10,
        };
        let body = json!({
            "nodes": [{ "_id": 1, "_label": "User" }, { "_id": 2, "_label": "User" }],
            "edges": [
                { "_id": 10, "_src": 1, "_dst": 2 },
                { "_id": 11, "_src": 1, "_dst": 99 },
                { "_id": 11, "_src": 1, "_dst": 99 }
            ]
        });
        let QueryResult::Graph { graph } = read_result(&body, &shape).unwrap() else {
            panic!("expected graph");
        };
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.dangling_edges, 1);
    }

    #[test]
    fn preserves_large_property_integers_as_strings_for_ipc() {
        let shape = ResultShape::Rows {
            variable: "rows".into(),
            columns: None,
            identity_variable: None,
        };
        let QueryResult::Rows { rows, .. } = read_result(
            &json!({ "rows": [{ "$id": 9223372036854775807_i64, "external": 9223372036854775807_i64 }] }),
            &shape,
        )
        .unwrap()
        else {
            panic!("expected rows");
        };
        assert_eq!(rows[0]["id"], json!("9223372036854775807"));
        assert_eq!(rows[0]["external"], json!("9223372036854775807"));
    }

    #[test]
    fn schema_rows_keep_large_property_integers_numeric() {
        let shape = ResultShape::Rows {
            variable: "rows".into(),
            columns: None,
            identity_variable: None,
        };
        let rows = decode_rows_for_schema(r#"{"rows":[{"external":9223372036854775807}]}"#, &shape)
            .unwrap();
        assert_eq!(rows[0]["external"], json!(9223372036854775807_i64));
    }

    #[test]
    fn reads_group_counts_from_samples() {
        let groups = read_group_count(&json!({
            "properties": [{ "$label": "User" }, { "$label": "Post" }, { "$label": "Post" }]
        }))
        .unwrap();
        assert_eq!(
            groups[0],
            LabelCount {
                label: "Post".into(),
                count: 2
            }
        );
    }

    #[test]
    fn describe_payload_stays_flat_for_the_typescript_contract() {
        let result = QueryResult::Describe {
            data: Box::new(DescribeResult {
                entity: "nodes",
                id: Some("1".into()),
                label: Some("User".into()),
                properties: Row::new(),
                edges: Vec::new(),
                neighbours: Vec::new(),
                degree: Some(0),
                endpoints: None,
            }),
        };
        let value = serde_json::to_value(result).unwrap();
        assert_eq!(value["kind"], "describe");
        assert_eq!(value["entity"], "nodes");
        assert_eq!(value["id"], "1");
        assert!(value.get("data").is_none());
    }
}
