//! Project status metadata and remembered suggestions; never determine membership.
use crate::data::{self, Ticket};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const CACHE_KEY: &str = "status-history-v1";
const MAX_NAMES: usize = 4096;
const MAX_NAME_BYTES: usize = 1024;

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct History {
    projects: BTreeMap<String, BTreeMap<String, String>>,
}
pub fn valid_project(project: &str) -> bool {
    project.len() <= 64 && data::valid_key(&format!("{project}-1"))
}
fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= MAX_NAME_BYTES && data::line(name) == name
}
/// User preferences, not a source of workflow states or allowed transitions.
/// Project keys are normalized; ambiguous duplicates and unsafe values fail closed.
pub fn normalize_orders(
    orders: BTreeMap<String, Vec<String>>,
) -> Result<BTreeMap<String, Vec<String>>> {
    ensure!(orders.len() <= 64, "Too many projects in status_order");
    let mut normalized = BTreeMap::new();
    for (project, order) in orders {
        ensure!(
            valid_project(&project),
            "Invalid project in status_order (value redacted)"
        );
        ensure!(order.len() <= 128, "Too many entries in status_order");
        let mut seen = BTreeSet::new();
        for name in &order {
            ensure!(
                valid_name(name),
                "Invalid status_order name (value redacted)"
            );
            ensure!(
                seen.insert(name.to_lowercase()),
                "Duplicate status_order name (value redacted)"
            );
        }
        ensure!(
            normalized.insert(project.to_uppercase(), order).is_none(),
            "Duplicate project in status_order (value redacted)"
        );
    }
    Ok(normalized)
}
/// Promote available forward steps in configured order. Stable sorting preserves
/// the caller's alphabetical fallback for backwards/unconfigured choices. Never
/// add a status, wrap at the end, or treat a preference as workflow permission.
pub fn prefer_next(names: &mut [String], current: &str, order: &[String]) {
    let current = current.to_lowercase();
    let Some(position) = order.iter().position(|s| s.to_lowercase() == current) else {
        return;
    };
    let forward: BTreeMap<_, _> = order
        .iter()
        .skip(position + 1)
        .enumerate()
        .map(|(n, name)| (name.to_lowercase(), n))
        .collect();
    names.sort_by_cached_key(|name| {
        forward
            .get(&name.to_lowercase())
            .copied()
            .unwrap_or(usize::MAX)
    });
}
/// Jira returns statuses grouped by issue type, including unused workflow states.
pub fn project_names(metadata: &serde_json::Value) -> Result<Vec<String>> {
    let types = metadata
        .as_array()
        .context("Invalid project status metadata")?;
    ensure!(
        types.len() <= MAX_NAMES,
        "Project status metadata exceeds limit"
    );
    let mut names = BTreeMap::new();
    for issue_type in types {
        let statuses = issue_type["statuses"]
            .as_array()
            .context("Invalid project status metadata")?;
        ensure!(
            statuses.len() <= MAX_NAMES,
            "Project status metadata exceeds limit"
        );
        for status in statuses {
            let raw = status["name"]
                .as_str()
                .context("Invalid project status name")?;
            ensure!(
                raw.len() <= MAX_NAME_BYTES,
                "Project status name exceeds limit"
            );
            let name = data::line(raw);
            ensure!(valid_name(&name), "Invalid project status name");
            names.entry(name.to_lowercase()).or_insert(name);
            ensure!(
                names.len() <= MAX_NAMES,
                "Project status metadata exceeds limit"
            );
        }
    }
    Ok(names.into_values().collect())
}
impl History {
    pub fn valid(&self) -> bool {
        self.projects.len() <= MAX_NAMES
            && self.projects.values().map(BTreeMap::len).sum::<usize>() <= MAX_NAMES
            && self.projects.iter().all(|(project, names)| {
                valid_project(project)
                    && !names.is_empty()
                    && names
                        .iter()
                        .all(|(key, name)| valid_name(name) && *key == name.to_lowercase())
            })
    }
    pub fn remember(&mut self, project: &str, name: &str) -> bool {
        let project = project.to_uppercase();
        let name = data::line(name);
        if !valid_project(&project) || !valid_name(&name) {
            return false;
        }
        let key = name.to_lowercase();
        if self
            .projects
            .get(&project)
            .is_some_and(|names| names.contains_key(&key))
            || self.projects.values().map(BTreeMap::len).sum::<usize>() >= MAX_NAMES
        {
            return false;
        }
        self.projects.entry(project).or_default().insert(key, name);
        true
    }
    pub fn register<'a>(&mut self, tickets: impl IntoIterator<Item = &'a Ticket>) -> bool {
        let mut changed = false;
        for ticket in tickets {
            if !data::valid_key(&ticket.key) {
                continue;
            }
            let Some((project, _)) = ticket.key.rsplit_once('-') else {
                continue;
            };
            changed |= self.remember(project, &ticket.status());
            for transition in &ticket.transitions {
                changed |= self.remember(project, &data::text(&transition["to"]["name"]));
            }
        }
        changed
    }
    pub fn names(&self, project: &str) -> Vec<String> {
        self.projects
            .get(&project.to_uppercase())
            .map(|names| names.values().cloned().collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn natural_order_promotes_only_available_forward_steps_without_wrapping() {
        let order: Vec<String> = [
            "To Do",
            "In Progress",
            "Code Review",
            "Staging",
            "PO Testing",
            "Production",
            "Done",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        let mut alphabetical = order.clone();
        alphabetical.extend(["Discarded".into(), "On Hold".into()]);
        alphabetical.sort();
        for pair in order.windows(2) {
            let mut choices = alphabetical.clone();
            prefer_next(&mut choices, &pair[0], &order);
            assert_eq!(choices[0], pair[1]);
            let mut inventory = choices.clone();
            inventory.sort();
            assert_eq!(inventory, alphabetical);
        }
        for current in ["Unknown state", "Done"] {
            let mut choices = alphabetical.clone();
            prefer_next(&mut choices, current, &order);
            assert_eq!(choices, alphabetical);
        }
        let mut choices = vec!["Done".into(), "On Hold".into(), "PO Testing".into()];
        prefer_next(&mut choices, "In Progress", &order);
        assert_eq!(choices, ["PO Testing", "Done", "On Hold"]);
        assert!(!choices.iter().any(|s| s == "Code Review"));
        let mut backwards = vec!["Discarded".into(), "In Progress".into(), "On Hold".into()];
        let original = backwards.clone();
        prefer_next(&mut backwards, "Production", &order);
        assert_eq!(backwards, original);
    }
    #[test]
    fn workflow_matching_is_case_insensitive_and_preserves_server_values() {
        let mut choices = vec!["Done".into(), "ÜBERPRÜFEN".into()];
        prefer_next(
            &mut choices,
            "IN PROGRESS",
            &["In Progress".into(), "Überprüfen".into(), "done".into()],
        );
        assert_eq!(choices, ["ÜBERPRÜFEN", "Done"]);
    }
    #[test]
    fn workflow_configuration_is_bounded_normalized_and_rejects_ambiguity() {
        let orders = normalize_orders(BTreeMap::from([
            (
                "dev".into(),
                vec!["In Progress".into(), "Code Review".into()],
            ),
            ("OTHER".into(), vec![]),
        ]))
        .unwrap();
        assert!(orders.contains_key("DEV"));
        assert!(orders["OTHER"].is_empty());
        for order in [
            vec!["".into()],
            vec![" In Progress".into()],
            vec!["Unsafe\nstatus".into()],
            vec!["\u{1b}[31mUnsafe".into()],
            vec!["Review".into(), "review".into()],
            vec!["x".repeat(MAX_NAME_BYTES + 1)],
            vec!["status".into(); 129],
        ] {
            assert!(normalize_orders(BTreeMap::from([("DEV".into(), order)])).is_err());
        }
        assert!(
            normalize_orders(BTreeMap::from([
                ("DEV".into(), vec![]),
                ("dev".into(), vec![])
            ]))
            .is_err()
        );
        assert!(normalize_orders(BTreeMap::from([("invalid project".into(), vec![])])).is_err());
        assert!(normalize_orders((0..65).map(|n| (format!("DEV{n}"), vec![])).collect()).is_err());
    }
    #[test]
    fn project_metadata_includes_unused_statuses_and_deduplicates_issue_types() {
        let names = project_names(&json!([
            {"name":"Task", "statuses":[{"name":"To Do"},{"name":"Discarded"}]},
            {"name":"Bug", "statuses":[{"name":"To Do"},{"name":"Ready for Staging"},
                {"name":"\u{1b}[31mDISCARDED"}]}
        ]))
        .unwrap();
        assert_eq!(names, ["Discarded", "Ready for Staging", "To Do"]);
        assert_eq!(project_names(&json!([])).unwrap(), Vec::<String>::new());
        for invalid in [
            json!({}),
            json!([{}]),
            json!([{"statuses":null}]),
            json!([{"statuses":[{"name":12}]}]),
            json!([{"statuses":[{"name":" "}]}]),
            json!([{"statuses":[{"name":"a".repeat(MAX_NAME_BYTES + 1)}]}]),
        ] {
            assert!(project_names(&invalid).is_err());
        }
    }
    #[test]
    fn remembers_sanitized_names_case_insensitively_per_project() {
        let mut history = History::default();
        assert!(history.remember("dev", " Ready for Staging\u{1b}[31m "));
        assert!(!history.remember("DEV", "READY FOR STAGING"));
        assert!(history.remember("OTHER", "Other workflow"));
        assert_eq!(history.names("DEV"), ["Ready for Staging"]);
        assert_eq!(history.names("OTHER"), ["Other workflow"]);
        assert!(history.names("UNKNOWN").is_empty());
        assert!(history.valid());
        let restored: History =
            serde_json::from_value(serde_json::to_value(&history).unwrap()).unwrap();
        assert_eq!(restored.names("dev"), history.names("DEV"));
    }

    #[test]
    fn learns_ticket_status_and_transition_targets_without_guessing() {
        let ticket: Ticket = serde_json::from_value(json!({
            "key": "DEV-1", "fields": {"status": {"name": "In Progress"}},
            "transitions": [{"to": {"name": "Ready for Staging"}}, {"name": "Not a status"}]
        }))
        .unwrap();
        let mut history = History::default();
        assert!(history.register([&ticket]));
        assert!(!history.register([&ticket]));
        assert_eq!(history.names("DEV"), ["In Progress", "Ready for Staging"]);
    }

    #[test]
    fn rejects_invalid_or_unbounded_persisted_metadata() {
        let mut history = History::default();
        assert!(!history.remember("bad project", "Ready"));
        assert!(!history.remember("DEV", ""));
        assert!(!history.remember("DEV", &"a".repeat(MAX_NAME_BYTES + 1)));
        for n in 0..MAX_NAMES {
            assert!(history.remember("DEV", &format!("Status {n}")));
        }
        assert!(!history.remember("DEV", "One too many"));
        assert!(history.valid());
        history
            .projects
            .get_mut("DEV")
            .unwrap()
            .insert("unsafe".into(), "\u{1b}[31mUnsafe".into());
        assert!(!history.valid());
    }
}
