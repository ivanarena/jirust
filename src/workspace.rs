//! One account-scoped dataset, shared by all three views.
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
    pub complete: bool,
    pub synced_at: u64,
    /// Derived once on load/sync, never serialized or rebuilt per keystroke.
    #[serde(skip)]
    pub(crate) index: HashMap<String, SearchRow>,
}
#[derive(Clone)]
pub(crate) struct SearchRow {
    pub position: usize,
    status: String,
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
            "{} {} {} {} {} {} ",
            ticket.key,
            ticket.summary(),
            ticket.status(),
            user.label(),
            ticket.user("reporter").label(),
            ticket.fields["labels"]
        );
        description(&ticket.fields["description"], &mut text);
        let assignees = vec![user.id, user.name, user.email, user.login];
        for value in &assignees {
            text.push_str(value);
            text.push(' ')
        }
        Self {
            position,
            status: ticket.status().to_lowercase(),
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
        assignee: &str,
        status: &str,
        words: &[&str],
    ) -> bool {
        if !self.backlog_ticket
            || view == config::MINE && !mine
            || view == config::AVAILABLE && !self.unassigned
        {
            return false;
        }
        if view == config::BACKLOG && !assignee.is_empty() {
            let matches = match assignee {
                "@me" => mine,
                "unassigned" => self.unassigned,
                value => self.assignees.iter().any(|s| s == value),
            };
            if !matches {
                return false;
            }
        }
        (status.is_empty() || self.status == status)
            && words.iter().all(|word| self.text.contains(word))
    }
}
impl Workspace {
    pub fn reindex(&mut self) {
        self.index = self
            .items
            .iter()
            .enumerate()
            .map(|(position, t)| (t.key.clone(), SearchRow::new(t, position)))
            .collect();
    }
    pub fn ticket(&self, key: &str) -> Option<&Ticket> {
        self.index
            .get(key)
            .and_then(|row| self.items.get(row.position))
            .or_else(|| self.items.iter().find(|t| t.key == key))
    }
    pub fn matches_words(
        &self,
        t: &Ticket,
        view: usize,
        assignee: &str,
        status: &str,
        words: &[&str],
    ) -> bool {
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
            &assignee.to_lowercase(),
            &status.to_lowercase(),
            &words,
        )
    }
    pub fn update(&mut self, ticket: &Ticket) {
        if let Some(position) = self
            .index
            .get(&ticket.key)
            .map(|row| row.position)
            .or_else(|| self.items.iter().position(|t| t.key == ticket.key))
        {
            let row = &mut self.items[position];
            for field in [
                "summary",
                "status",
                "assignee",
                "reporter",
                "priority",
                "issuetype",
                "labels",
                "description",
            ] {
                row.fields[field] = ticket.fields[field].clone();
            }
            self.index
                .insert(row.key.clone(), SearchRow::new(row, position));
        }
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
    items.iter().all(|t| data::valid_key(&t.key))
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn changed_query_preserves_sort() {
        let q = Workspace::recent_query("project = DEV ORDER BY Rank ASC", 10);
        assert!(q.contains("updated >= -10m"));
        assert!(q.ends_with("ORDER BY Rank ASC"));
    }
}
