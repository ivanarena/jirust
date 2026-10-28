//! One account-scoped dataset, shared by all four views.
use crate::{
    config,
    data::{self, Ticket},
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Workspace {
    pub items: Vec<Ticket>,
    pub mine: HashSet<String>,
    /// Exact API membership. None denotes a legacy, unverified scope.
    #[serde(default)]
    pub board: Option<HashSet<String>>,
    pub complete: bool,
    pub synced_at: u64,
    /// Derived once on load/sync, never serialized or rebuilt per keystroke.
    #[serde(skip)]
    pub(crate) index: HashMap<String, SearchRow>,
    #[serde(skip)]
    pub(crate) encoded_bytes: usize,
}
#[derive(Clone)]
pub(crate) struct SearchRow {
    pub position: usize,
    encoded_bytes: usize,
    status: String,
    epic: Option<String>,
    assignees: Vec<String>,
    unassigned: bool,
    backlog_ticket: bool,
    text: String,
}
impl SearchRow {
    fn new(ticket: &Ticket, position: usize) -> Self {
        // Search does not need Markdown formatting. Walk ADF text once, without
        // repeatedly escaping/concatenating entire document subtrees.
        fn description(value: &serde_json::Value, out: &mut String) {
            if let Some(text) = value.as_str() {
                out.push_str(text);
                out.push(' ');
                return;
            }
            if let Some(text) = value["text"].as_str() {
                out.push_str(text);
            }
            for attr in ["text", "url", "href"] {
                if let Some(text) = value["attrs"][attr].as_str() {
                    out.push_str(text);
                    out.push(' ')
                }
            }
            for child in value["content"].as_array().into_iter().flatten() {
                description(child, out)
            }
            if !matches!(
                value["type"].as_str(),
                Some("text" | "mention" | "emoji" | "inlineCard")
            ) {
                out.push(' ');
            }
        }
        let user = ticket.user("assignee");
        let mut text = format!(
            "{} {} {} {} {} {} {} ",
            ticket.key,
            ticket.kind(),
            ticket.summary(),
            ticket.status(),
            user.label(),
            ticket.user("reporter").label(),
            ticket.fields["labels"]
        );
        if let Some(epic) = ticket.parent_epic() {
            text.push_str(&epic.key);
            text.push(' ');
            text.push_str(&epic.name);
            text.push(' ');
        }
        description(&ticket.fields["description"], &mut text);
        let assignees = vec![user.id, user.name, user.email, user.login];
        for value in &assignees {
            text.push_str(value);
            text.push(' ')
        }
        Self {
            position,
            encoded_bytes: crate::limits::size(ticket, crate::limits::WORKSPACE_BYTES)
                .unwrap_or(crate::limits::WORKSPACE_BYTES + 1),
            status: ticket.status().to_lowercase(),
            epic: if ticket.is_epic() {
                Some(ticket.key.to_lowercase())
            } else {
                ticket.parent_epic().map(|p| p.key.to_lowercase())
            },
            assignees: assignees.into_iter().map(|s| s.to_lowercase()).collect(),
            unassigned: ticket.fields["assignee"].is_null(),
            backlog_ticket: ticket.backlog_ticket(),
            text: text.to_lowercase(),
        }
    }
    fn matches(
        &self,
        mine: bool,
        view: usize,
        assignee: &[String],
        status: &[String],
        words: &[&str],
    ) -> bool {
        if !self.backlog_ticket
            || view == config::MINE && !mine
            || view == config::AVAILABLE && !self.unassigned
        {
            return false;
        }
        if !assignee.is_empty()
            && !assignee.iter().any(|value| match value.as_str() {
                "@me" => mine,
                "unassigned" => self.unassigned,
                value => {
                    if let Some((tag, value)) = value.split_once(':') {
                        let index = match tag {
                            "@id" => Some(0),
                            "@name" => Some(1),
                            "@email" => Some(2),
                            "@login" => Some(3),
                            _ => None,
                        };
                        if let Some(index) = index {
                            return !value.is_empty() && self.assignees[index] == value;
                        }
                    }
                    self.assignees.iter().any(|s| s == value)
                }
            })
        {
            return false;
        }
        (status.is_empty() || status.contains(&self.status))
            && words.iter().all(|word| self.text.contains(word))
    }
}
impl Workspace {
    /// Copy only persisted fields, not the large derived search index.
    pub fn cache_snapshot(&self) -> Self {
        Self {
            items: self.items.clone(),
            mine: self.mine.clone(),
            board: self.board.clone(),
            complete: self.complete,
            synced_at: self.synced_at,
            ..Default::default()
        }
    }
    pub fn reindex(&mut self) {
        self.index = self
            .items
            .iter()
            .enumerate()
            .map(|(position, t)| (t.key.clone(), SearchRow::new(t, position)))
            .collect();
        self.encoded_bytes = crate::limits::size(self, crate::limits::WORKSPACE_BYTES)
            .unwrap_or(crate::limits::WORKSPACE_BYTES + 1);
    }
    pub fn ticket(&self, key: &str) -> Option<&Ticket> {
        self.index
            .get(key)
            .and_then(|row| self.items.get(row.position))
            .or_else(|| self.items.iter().find(|t| t.key == key))
    }
    pub fn matches_epic(&self, t: &Ticket, key: Option<&str>) -> bool {
        key.is_none_or(|key| {
            self.index
                .get(&t.key)
                .and_then(|r| r.epic.as_deref())
                .is_some_and(|epic| epic.eq_ignore_ascii_case(key))
        })
    }
    pub fn matches_words(
        &self,
        t: &Ticket,
        view: usize,
        assignee: &[String],
        status: &[String],
        words: &[&str],
    ) -> bool {
        let is_board = self
            .board
            .as_ref()
            .is_some_and(|keys| keys.contains(&t.key));
        if view == config::BOARD && !is_board || view == config::BACKLOG && is_board {
            return false;
        }
        if let Some(row) = self.index.get(&t.key) {
            row.matches(self.mine.contains(&t.key), view, assignee, status, words)
        } else {
            SearchRow::new(t, 0).matches(self.mine.contains(&t.key), view, assignee, status, words)
        }
    }
    pub fn matches(
        &self,
        t: &Ticket,
        view: usize,
        assignee: &str,
        status: &str,
        search: &str,
    ) -> bool {
        let search = search.to_lowercase();
        let words: Vec<_> = search.split_whitespace().collect();
        self.matches_words(
            t,
            view,
            &crate::filters::Selection::from(assignee).normalized(),
            &crate::filters::Selection::from(status).normalized(),
            &words,
        )
    }
    /// Reject budget-expanding detail updates without mutating the useful snapshot.
    pub fn update(&mut self, ticket: &Ticket) -> bool {
        self.update_with_limit(ticket, crate::limits::WORKSPACE_BYTES)
    }
    fn update_with_limit(&mut self, ticket: &Ticket, limit: usize) -> bool {
        if self.index.len() != self.items.len() || self.encoded_bytes == 0 {
            self.reindex();
        }
        if let Some(position) = self
            .index
            .get(&ticket.key)
            .map(|row| row.position)
            .or_else(|| self.items.iter().position(|t| t.key == ticket.key))
        {
            let mut row = self.items[position].clone();
            for field in [
                "summary",
                "status",
                "assignee",
                "reporter",
                "priority",
                "issuetype",
                "labels",
                "description",
                "parent",
            ] {
                row.fields[field] = ticket.fields[field].clone();
            }
            row.update_estimates(ticket);
            let Ok(size) = crate::limits::size(&row, limit) else {
                return false;
            };
            let previous = self.index.get(&ticket.key).map_or(0, |r| r.encoded_bytes);
            let used = self
                .encoded_bytes
                .saturating_sub(previous)
                .saturating_add(size);
            if used > limit {
                return false;
            }
            self.index
                .insert(row.key.clone(), SearchRow::new(&row, position));
            self.items[position] = row;
            self.encoded_bytes = used;
        }
        true
    }
    /// Reflect a successful, confirmed write before the network reconciliation.
    /// Only explicit moves alter Board membership; status names never do.
    pub fn apply_confirmed(
        &mut self,
        action: &crate::backend::Action,
        jira: bool,
    ) -> Option<Ticket> {
        use serde_json::{Value, json};
        let mut ticket = self.ticket(&action.key)?.clone();
        let mut mine = None;
        match action.kind.as_str() {
            "assign" => {
                let known = self.items.iter().find(|t| {
                    if action.value == "@me" {
                        self.mine.contains(&t.key) && !t.fields["assignee"].is_null()
                    } else {
                        t.user("assignee").identity(jira) == action.value
                    }
                });
                mine = Some(
                    action.value == "@me" || known.is_some_and(|t| self.mine.contains(&t.key)),
                );
                ticket.fields["assignee"] = if action.value == "unassigned" {
                    mine = Some(false);
                    Value::Null
                } else if let Some(known) = known {
                    known.fields["assignee"].clone()
                } else if action.value == "@me" {
                    // No cached self identity: do not invent an account ID.
                    json!({"displayName":"Me"})
                } else {
                    let mut user = json!({"displayName":action.value});
                    user[if action.value.contains('@') {
                        "emailAddress"
                    } else if jira {
                        "name"
                    } else {
                        "accountId"
                    }] = json!(action.value);
                    user
                };
            }
            "status" => {
                ticket.fields["status"] = ticket
                    .transitions
                    .iter()
                    .map(|t| &t["to"])
                    .chain(self.items.iter().map(|t| &t.fields["status"]))
                    .find(|s| s["name"].as_str() == Some(&action.value))
                    .cloned()
                    .unwrap_or_else(|| json!({"name":action.value}));
            }
            "summary" => ticket.fields["summary"] = json!(action.value),
            "description" => ticket.fields["description"] = crate::markdown::to_adf(&action.value),
            "labels" => {
                ticket.fields["labels"] =
                    json!(action.value.split(',').map(str::trim).collect::<Vec<_>>())
            }
            "board" | "backlog" => {}
            _ => return None,
        }
        // Reserve bounded membership growth as well as the updated ticket bytes.
        // The next server snapshot replaces this conservative accounting.
        let reserve = 2 * (action.key.len() + 3);
        if !self.update_with_limit(
            &ticket,
            crate::limits::WORKSPACE_BYTES.saturating_sub(reserve),
        ) {
            return None;
        }
        if let Some(mine) = mine {
            if mine {
                self.mine.insert(action.key.clone());
            } else {
                self.mine.remove(&action.key);
            }
        }
        if let Some(board) = &mut self.board {
            match action.kind.as_str() {
                "board" => {
                    board.insert(action.key.clone());
                }
                "backlog" => {
                    board.remove(&action.key);
                }
                _ => {}
            }
        }
        self.encoded_bytes = self.encoded_bytes.saturating_add(reserve);
        Some(ticket)
    }
    /// True Jira/view membership count, independent of search and filters.
    pub fn view_count(&self, view: usize) -> usize {
        self.items
            .iter()
            .filter(|t| self.matches_words(t, view, &[], &[], &[]))
            .count()
    }
    pub fn recent_query(base: &str, minutes: u64) -> String {
        let (where_clause, order) = config::split_order(base);
        format!(
            "({where_clause}) AND updated >= -{}m {order}",
            minutes.max(2)
        )
    }
    pub fn keys(&self) -> HashSet<String> {
        self.items.iter().map(|t| t.key.clone()).collect()
    }
}
pub fn full_cache_key(key: &str) -> String {
    format!("detail-v2:{key}")
}
pub fn valid_items(items: &[Ticket]) -> bool {
    let mut keys = HashSet::new();
    items
        .iter()
        .all(|t| data::valid_key(&t.key) && keys.insert(&t.key))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn confirmed_assignment_updates_membership_and_reuses_known_identity() {
        use crate::backend::Action;
        let mut w = Workspace {
            items: data::demo_tickets(),
            mine: ["DEV-101".into()].into(),
            board: Some(["DEV-101".into()].into()),
            ..Default::default()
        };
        w.reindex();
        let assignee = w.ticket("DEV-101").unwrap().fields["assignee"].clone();
        w.apply_confirmed(&Action::new("assign", "DEV-102", "@me"), false)
            .unwrap();
        assert!(w.mine.contains("DEV-102"));
        assert_eq!(w.ticket("DEV-102").unwrap().fields["assignee"], assignee);
        assert_eq!(w.view_count(config::AVAILABLE), 0);
        w.apply_confirmed(&Action::new("assign", "DEV-102", "sam"), false)
            .unwrap();
        assert!(!w.mine.contains("DEV-102"));
        assert_eq!(w.ticket("DEV-102").unwrap().user("assignee").id, "sam");
        let id = w.ticket("DEV-101").unwrap().user("assignee").id;
        w.apply_confirmed(&Action::new("assign", "DEV-102", &id), false)
            .unwrap();
        assert!(w.mine.contains("DEV-102"));
        w.apply_confirmed(&Action::new("assign", "DEV-102", "unassigned"), false)
            .unwrap();
        assert!(!w.mine.contains("DEV-102"));
        assert_eq!(w.view_count(config::AVAILABLE), 1);
        assert_eq!(
            w.board.as_ref().unwrap(),
            &HashSet::from(["DEV-101".into()])
        );
    }
    #[test]
    fn confirmed_self_assignment_without_cached_identity_does_not_invent_account_id() {
        let mut w = Workspace {
            items: data::demo_tickets(),
            ..Default::default()
        };
        w.reindex();
        w.apply_confirmed(
            &crate::backend::Action::new("assign", "DEV-102", "@me"),
            false,
        )
        .unwrap();
        let row = w.ticket("DEV-102").unwrap();
        assert!(w.matches(row, config::MINE, "@me", "", ""));
        assert!(!w.matches(row, config::AVAILABLE, "", "", ""));
        assert!(row.user("assignee").id.is_empty());
    }
    #[test]
    fn confirmed_status_never_guesses_category_or_board_membership() {
        use crate::backend::Action;
        let mut w = Workspace {
            items: data::demo_tickets(),
            board: Some(["DEV-101".into()].into()),
            ..Default::default()
        };
        w.reindex();
        w.apply_confirmed(&Action::new("status", "DEV-102", "Unknown state"), false)
            .unwrap();
        assert_eq!(w.ticket("DEV-102").unwrap().status(), "Unknown state");
        assert!(w.ticket("DEV-102").unwrap().fields["status"]["statusCategory"].is_null());
        assert_eq!(
            w.board.as_ref().unwrap(),
            &HashSet::from(["DEV-101".into()])
        );
        w.apply_confirmed(&Action::new("board", "DEV-102", "1"), false)
            .unwrap();
        assert!(w.board.as_ref().unwrap().contains("DEV-102"));
        w.apply_confirmed(&Action::new("backlog", "DEV-102", "1"), false)
            .unwrap();
        assert!(!w.board.as_ref().unwrap().contains("DEV-102"));
        assert_eq!(w.ticket("DEV-102").unwrap().status(), "Unknown state");
    }
    #[test]
    fn full_detail_updates_cannot_grow_the_shared_snapshot_past_its_budget() {
        let mut w = Workspace {
            items: data::demo_tickets(),
            ..Default::default()
        };
        w.reindex();
        let before = w.items[0].summary();
        let limit = w.encoded_bytes + 16;
        let mut detail = w.items[0].clone();
        detail.fields["summary"] = serde_json::json!("x".repeat(1024));
        assert!(!w.update_with_limit(&detail, limit));
        assert_eq!(w.items[0].summary(), before);
        assert!(w.matches(&w.items[0], config::BACKLOG, "", "", &before));
        detail.fields["summary"] = serde_json::json!("small");
        assert!(w.update_with_limit(&detail, limit));
    }
    #[test]
    fn parent_epic_labels_update_search_and_clear_without_changing_rank_or_membership() {
        let items = data::demo_tickets();
        let mut w = Workspace {
            items,
            ..Default::default()
        };
        w.reindex();
        let keys = w.items.iter().map(|t| t.key.clone()).collect::<Vec<_>>();
        let mut t = w.items[0].clone();
        t.fields["parent"] = serde_json::json!({"key":"DEV-900","fields":{"summary":"Roadmap","issuetype":{"name":"Epic"}}});
        w.update(&t);
        assert_eq!(w.items[0].parent_epic().unwrap().name, "Roadmap");
        assert!(w.matches(&w.items[0], config::BACKLOG, "", "", "Roadmap"));
        assert!(w.matches(&w.items[0], config::BACKLOG, "", "", "DEV-900"));
        assert_eq!(
            w.items.iter().map(|t| t.key.clone()).collect::<Vec<_>>(),
            keys
        );
        t.fields["parent"] = serde_json::Value::Null;
        w.update(&t);
        assert!(w.items[0].parent_epic().is_none());
        assert!(!w.matches(&w.items[0], config::BACKLOG, "", "", "Roadmap"));
        assert_eq!(
            w.items.iter().map(|t| t.key.clone()).collect::<Vec<_>>(),
            keys
        );
    }
    #[test]
    fn epics_are_searchable_and_obey_actual_membership_and_rank() {
        let mut w = Workspace {
            items: data::demo_tickets(),
            board: Some(["DEV-101".into()].into()),
            mine: ["DEV-101".into()].into(),
            ..Default::default()
        };
        w.items[0].fields["issuetype"] = serde_json::json!({"name":"Epic","hierarchyLevel":1});
        w.items[1].fields["issuetype"] = serde_json::json!({"name":"Epic","hierarchyLevel":1});
        w.reindex();
        assert!(w.matches(&w.items[0], config::BOARD, "", "", "epic"));
        assert!(!w.matches(&w.items[0], config::BACKLOG, "", "", "epic"));
        assert!(w.matches(&w.items[1], config::BACKLOG, "", "", "epic"));
        assert!(w.matches(&w.items[1], config::AVAILABLE, "", "", "epic"));
        assert!(!w.matches(&w.items[1], config::MINE, "", "", "epic"));
        assert_eq!(w.view_count(config::BACKLOG), 2);
        assert_eq!(
            w.items.iter().map(|t| t.key.as_str()).collect::<Vec<_>>(),
            ["DEV-101", "DEV-102", "DEV-103"]
        );
    }
    #[test]
    fn multi_filters_intersect_preset_membership_and_preserve_rank_order() {
        let mut items = data::demo_tickets();
        items[0].key = "DEV-84".into();
        items[1].key = "DEV-9".into();
        items[2].key = "DEV-61".into();
        let mut w = Workspace {
            items,
            mine: ["DEV-84".into()].into(),
            board: Some(["DEV-84".into(), "DEV-61".into()].into()),
            ..Default::default()
        };
        w.reindex();
        let assignees = vec!["@me".into(), "sam".into()];
        let statuses = vec!["in progress".into(), "to do".into()];
        let keys = |view| {
            w.items
                .iter()
                .filter(|t| w.matches_words(t, view, &assignees, &statuses, &[]))
                .map(|t| t.key.as_str())
                .collect::<Vec<_>>()
        };
        assert_eq!(keys(config::BOARD), ["DEV-84", "DEV-61"]);
        assert!(keys(config::BACKLOG).is_empty());
        assert_eq!(keys(config::MINE), ["DEV-84"]);
        assert!(w.matches_words(
            &w.items[1],
            config::AVAILABLE,
            &["unassigned".into(), "sam".into()],
            &[],
            &[]
        ));
        assert!(!w.matches_words(&w.items[0], config::AVAILABLE, &assignees, &statuses, &[]));
    }
    #[test]
    fn local_filters_use_server_membership_and_entire_descriptions() {
        let mut w = Workspace {
            items: crate::data::demo_tickets(),
            mine: HashSet::from(["DEV-101".into()]),
            complete: true,
            ..Default::default()
        };
        w.reindex();
        assert!(w.matches(&w.items[0], config::MINE, "", "", "fast navigation"));
        assert!(!w.matches(&w.items[1], config::MINE, "", "", ""));
        assert!(w.matches(&w.items[1], config::AVAILABLE, "", "", "no enter"));
        assert!(w.matches(&w.items[2], config::BACKLOG, "sam", "To Do", "confirmation"));
    }
    #[test]
    fn index_updates_one_ticket_and_does_not_change_server_order() {
        let mut w = Workspace {
            items: crate::data::demo_tickets(),
            ..Default::default()
        };
        w.items.reverse();
        w.reindex();
        let mut ticket = w.items[0].clone();
        ticket.fields["description"] = serde_json::json!("new needle");
        w.update(&ticket);
        assert!(w.matches(&w.items[0], config::BACKLOG, "", "", "new needle"));
        assert_eq!(w.items[0].key, "DEV-103");
        assert_eq!(w.ticket("DEV-101").unwrap().key, "DEV-101");
        assert!(!serde_json::to_string(&w).unwrap().contains("\"index\""));
    }
    #[test]
    fn indexed_adf_search_joins_inline_marks_and_separates_blocks() {
        let mut w = Workspace {
            items: crate::data::demo_tickets(),
            ..Default::default()
        };
        w.items[0].fields["description"] = serde_json::json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"check","marks":[{"type":"strong"}]},{"type":"text","text":"list"}]},{"type":"paragraph","content":[{"type":"text","text":"separate"}]}]});
        w.reindex();
        assert!(w.matches(&w.items[0], config::BACKLOG, "", "", "checklist separate"));
        assert!(!w.matches(&w.items[0], config::BACKLOG, "", "", "listseparate"));
    }
    #[test]
    fn estimate_hydration_keeps_rank_and_does_not_turn_zero_into_unknown() {
        let mut w = Workspace {
            items: data::demo_tickets(),
            ..Default::default()
        };
        w.reindex();
        let keys = w.items.iter().map(|t| t.key.clone()).collect::<Vec<_>>();
        let mut t = w.items[0].clone();
        t.fields["customfield_90123"] = serde_json::json!(0);
        w.update(&t);
        assert_eq!(w.items[0].points_label(), "0");
        assert_eq!(
            keys,
            w.items.iter().map(|t| t.key.clone()).collect::<Vec<_>>()
        );
        t.fields["customfield_90123"] = serde_json::Value::Null;
        w.update(&t);
        assert_eq!(w.items[0].points_label(), "");
    }
    #[test]
    fn duplicate_page_keys_are_rejected_instead_of_corrupting_search_positions() {
        let mut t = data::demo_tickets();
        assert!(valid_items(&t));
        t.push(t[0].clone());
        assert!(!valid_items(&t));
    }
    #[test]
    fn changed_query_preserves_sort() {
        let q = Workspace::recent_query("project = DEV ORDER BY Rank ASC", 10);
        assert!(q.contains("updated >= -10m"));
        assert!(q.ends_with("ORDER BY Rank ASC"));
    }
}
