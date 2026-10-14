//! Optional local view order. Never mutate membership, Jira Rank or the dataset.
use crate::data::Ticket;
use std::collections::{BTreeMap, HashSet};

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub enum Order {
    #[default]
    Rank,
    Epic,
}
impl Order {
    pub fn label(self) -> &'static str {
        match self {
            Self::Rank => "Rank",
            Self::Epic => "Epic",
        }
    }
    pub fn toggled(self) -> Self {
        match self {
            Self::Rank => Self::Epic,
            Self::Epic => Self::Rank,
        }
    }
}
/// Build once on dataset/parent changes, not during each search keystroke.
/// Board first; parent-epic name/key next; no parent last; stable Rank within groups.
pub fn epic_order(items: &[Ticket], board: Option<&HashSet<String>>) -> Vec<usize> {
    let parents: Vec<_> = items.iter().map(Ticket::parent_epic).collect();
    let mut names: BTreeMap<String, String> = BTreeMap::new();
    for parent in parents.iter().flatten() {
        let key = parent.key.to_uppercase();
        let name = parent.name.to_lowercase();
        names
            .entry(key)
            .and_modify(|known| {
                if name < *known {
                    known.clone_from(&name);
                }
            })
            .or_insert(name);
    }
    // An explicitly known epic's current title wins over stale parent summaries.
    for ticket in items.iter().filter(|t| t.is_epic()) {
        let name = crate::data::line(&ticket.summary());
        if !name.is_empty() {
            names.insert(ticket.key.to_uppercase(), name.to_lowercase());
        }
    }
    let keys: Vec<_> = items
        .iter()
        .zip(&parents)
        .map(|(ticket, parent)| {
            let parent = parent.as_ref().map(|p| p.key.to_uppercase());
            let label = parent.as_ref().and_then(|key| names.get(key)).cloned();
            (
                board.is_some_and(|b| !b.contains(&ticket.key)),
                parent.is_none(),
                label,
                parent,
            )
        })
        .collect();
    let mut order: Vec<_> = (0..items.len()).collect();
    order.sort_by(|a, b| keys[*a].cmp(&keys[*b])); // stable: source Rank breaks all ties
    order
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn ticket(key: &str, epic: Option<(&str, &str)>) -> Ticket {
        let mut t = crate::data::demo_tickets()[0].clone();
        t.key = key.into();
        t.fields["parent"] = epic.map(|(key,name)| json!({"key":key,"fields":{"summary":name,"issuetype":{"name":"Epic"}}})).unwrap_or_default();
        t
    }
    #[test]
    fn epic_names_group_locally_without_losing_section_or_rank_or_moving_unassigned_first() {
        let items = vec![
            ticket("DEV-3", Some(("DEV-901", "Zulu"))),
            ticket("DEV-8", None),
            ticket("DEV-9", Some(("DEV-900", "alpha"))),
            ticket("DEV-1", Some(("DEV-900", "alpha"))),
            ticket("DEV-2", Some(("DEV-900", "alpha"))),
            ticket("DEV-4", None),
        ];
        let board = HashSet::from([
            "DEV-3".into(),
            "DEV-8".into(),
            "DEV-9".into(),
            "DEV-1".into(),
        ]);
        let order = epic_order(&items, Some(&board));
        assert_eq!(
            order
                .iter()
                .map(|i| items[*i].key.as_str())
                .collect::<Vec<_>>(),
            ["DEV-9", "DEV-1", "DEV-3", "DEV-8", "DEV-2", "DEV-4"]
        );
        assert_eq!(items[0].key, "DEV-3");
        assert_eq!(items[3].key, "DEV-1");
    }
    #[test]
    fn same_key_stays_one_group_across_renames_and_epic_rows_have_no_inferred_parent() {
        let mut items = vec![
            ticket("DEV-1", Some(("DEV-900", "old name"))),
            ticket("DEV-2", Some(("DEV-901", "middle"))),
            ticket("DEV-3", Some(("dev-900", "new name"))),
        ];
        let mut epic = ticket("DEV-900", None);
        epic.fields["issuetype"] = json!({"name":"Epic"});
        epic.fields["summary"] = json!("Zulu current title");
        items.push(epic);
        assert_eq!(epic_order(&items, None), [1, 0, 2, 3]);
        items[0].fields["parent"] =
            json!({"key":"DEV-777","fields":{"summary":"not an epic","issuetype":{"name":"Task"}}});
        assert_eq!(epic_order(&items, None), [1, 2, 0, 3]);
    }
}
