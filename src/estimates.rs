//! Story-point IDs come from approved server field metadata, never fixed IDs.
use serde_json::Value;
pub fn valid_field(id: &str) -> bool {
    id.strip_prefix("customfield_").is_some_and(|n| {
        !n.is_empty()
            && n.len() <= 20
            && n.bytes().all(|b| b.is_ascii_digit())
            && n.parse::<u64>().is_ok_and(|n| n > 0)
    })
}
pub fn point_name(name: &str) -> bool {
    matches!(
        name.trim().to_lowercase().as_str(),
        "story points" | "story point estimate" | "story points estimate"
    )
}
pub fn fields(metadata: &Value) -> anyhow::Result<Vec<String>> {
    use anyhow::{Context, ensure};
    let rows = metadata.as_array().context("Invalid Jira field metadata")?;
    ensure!(rows.len() <= 10000, "Jira field metadata exceeds limit");
    let mut ids = std::collections::BTreeSet::new();
    for row in rows {
        let id = row["id"].as_str().unwrap_or("");
        if valid_field(id)
            && row["schema"]["type"] == "number"
            && (point_name(row["name"].as_str().unwrap_or(""))
                || row["schema"]["custom"] == "com.pyxis.greenhopper.jira:jsw-story-points")
        {
            ids.insert(id.to_owned());
        }
    }
    ensure!(
        ids.len() <= 32,
        "Too many story-point field candidates; metadata is ambiguous"
    );
    Ok(ids.into_iter().collect())
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn discovery_uses_numeric_schema_and_server_names_not_customfield_numbers() {
        let v = json!([
            {"id":"customfield_90123","name":"Story point estimate","schema":{"type":"number"}},
            {"id":"customfield_90234","name":"Localized name","schema":{"type":"number","custom":"com.pyxis.greenhopper.jira:jsw-story-points"}},
            {"id":"customfield_10016","name":"Unrelated counter","schema":{"type":"number"}},
            {"id":"customfield_90345","name":"Story Points","schema":{"type":"string"}},
            {"id":"customfield_1/../../","name":"Story Points","schema":{"type":"number"}}
        ]);
        assert_eq!(
            fields(&v).unwrap(),
            ["customfield_90123", "customfield_90234"]
        );
        assert!(fields(&json!({})).is_err());
    }
}
