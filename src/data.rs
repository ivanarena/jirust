use anyhow::{Result, bail};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::LazyLock;
use url::Url;

static KEY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z][A-Za-z0-9_]*-[0-9]+$").unwrap());
pub static URLS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"https?://[^\s<>\"']+"#).unwrap());
pub static KEYS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b[A-Z][A-Z0-9_]*-[0-9]+\b").unwrap());

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Ticket {
    #[serde(default)]
    pub key: String,
    #[serde(default, rename = "self", deserialize_with = "null_default")]
    pub self_url: String,
    #[serde(default)]
    pub fields: Value,
    #[serde(default)]
    pub names: Value,
    #[serde(default)]
    pub estimate_fields: Vec<String>,
    #[serde(default, deserialize_with = "null_default")]
    pub transitions: Vec<Value>,
}
fn null_default<'de, D, T>(d: D) -> std::result::Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}
impl Ticket {
    pub fn summary(&self) -> String {
        text(&self.fields["summary"])
    }
    pub fn status(&self) -> String {
        text(&self.fields["status"]["name"])
    }
    pub fn user(&self, field: &str) -> User {
        User::from_value(&self.fields[field])
    }
    pub fn story_points(&self) -> Option<f64> {
        let mut ids = self
            .estimate_fields
            .iter()
            .filter(|id| crate::estimates::valid_field(id))
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>();
        if let Some(names) = self.names.as_object() {
            ids.extend(
                names
                    .iter()
                    .filter(|(id, name)| {
                        crate::estimates::valid_field(id)
                            && name.as_str().is_some_and(crate::estimates::point_name)
                    })
                    .map(|(id, _)| id.as_str()),
            );
        }
        // Multiple populated point fields are ambiguous; never sum or choose one.
        let values = ids
            .into_iter()
            .filter_map(|id| self.fields.get(id))
            .filter(|v| !v.is_null())
            .collect::<Vec<_>>();
        if values.len() != 1 {
            return None;
        }
        values[0]
            .as_f64()
            .filter(|n| n.is_finite() && *n >= 0. && *n <= 1_000_000.)
    }
    pub fn update_estimates(&mut self, other: &Ticket) {
        let mut ids = other
            .estimate_fields
            .iter()
            .filter(|id| crate::estimates::valid_field(id))
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        if let Some(names) = other.names.as_object() {
            ids.extend(
                names
                    .iter()
                    .filter(|(id, name)| {
                        crate::estimates::valid_field(id)
                            && name.as_str().is_some_and(crate::estimates::point_name)
                    })
                    .map(|(id, _)| id.clone()),
            );
        }
        for id in &ids {
            self.fields[id] = other.fields[id].clone();
        }
        self.estimate_fields = ids.into_iter().collect();
    }
    pub fn points_label(&self) -> String {
        self.story_points()
            .map(|n| n.to_string())
            .unwrap_or_else(|| "—".into())
    }
    pub fn priority(&self) -> String {
        text(&self.fields["priority"]["name"])
    }
    pub fn kind(&self) -> String {
        text(&self.fields["issuetype"]["name"])
    }
    pub fn is_epic(&self) -> bool {
        self.kind().eq_ignore_ascii_case("Epic")
            || self.fields["issuetype"]["hierarchyLevel"].as_i64() == Some(1)
    }
    /// Show standard tickets and epics, but keep child tasks in direct lookup.
    pub fn backlog_ticket(&self) -> bool {
        let kind = self.fields["issuetype"]["name"].as_str().unwrap_or("");
        !kind.eq_ignore_ascii_case("Sub-Task")
            && !kind.eq_ignore_ascii_case("Subtask")
            && !self.fields["issuetype"]["subtask"]
                .as_bool()
                .unwrap_or(false)
            && !self.fields["issuetype"]["hierarchyLevel"]
                .as_i64()
                .is_some_and(|level| level != 0 && level != 1)
    }
    pub fn description(&self) -> String {
        adf_markdown(&self.fields["description"])
    }
    pub fn web_url(&self) -> Option<String> {
        if !valid_key(&self.key) {
            return None;
        }
        let mut url = Url::parse(&self.self_url).ok()?;
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            return None;
        }
        let _ = url.set_username("");
        let _ = url.set_password(None);
        url.set_path(&format!("/browse/{}", self.key));
        url.set_query(None);
        url.set_fragment(None);
        Some(url.to_string())
    }
    pub fn attachments(&self) -> Vec<Attachment> {
        self.fields["attachment"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|v| Attachment {
                id: text(&v["id"]),
                name: text(&v["filename"]),
                mime: text(&v["mimeType"]),
                size: v["size"].as_u64().unwrap_or(0),
            })
            .collect()
    }
    pub fn targets(&self) -> Vec<Target> {
        let mut out = Vec::new();
        let mut add = |target: Target| {
            if !out.iter().any(|t: &Target| {
                t.key == target.key && t.url == target.url && t.attachment == target.attachment
            }) {
                out.push(target);
            }
        };
        if let Some(key) = self.fields["parent"]["key"].as_str() {
            add(Target::ticket(format!("Parent · {key}"), key));
        }
        for sub in self.fields["subtasks"].as_array().into_iter().flatten() {
            let key = text(&sub["key"]);
            if valid_key(&key) {
                add(Target::ticket(format!("Subtask · {key}"), &key));
            }
        }
        for link in self.fields["issuelinks"].as_array().into_iter().flatten() {
            for (dir, label) in [("inwardIssue", "inward"), ("outwardIssue", "outward")] {
                let key = text(&link[dir]["key"]);
                if valid_key(&key) {
                    add(Target::ticket(
                        format!(
                            "{} · {key} {}",
                            text(&link["type"][label]),
                            text(&link[dir]["fields"]["summary"])
                        ),
                        &key,
                    ));
                }
            }
        }
        for found in KEYS.find_iter(&self.description()) {
            if found.as_str() != self.key {
                add(Target::ticket(
                    format!("Mention · {}", found.as_str()),
                    found.as_str(),
                ));
            }
        }
        fn urls(v: &Value, out: &mut Vec<String>) {
            match v {
                Value::Array(a) => {
                    for x in a {
                        urls(x, out)
                    }
                }
                Value::Object(m) => {
                    if let Some(a) = m.get("attrs") {
                        for k in ["href", "url"] {
                            if let Some(s) = a[k].as_str()
                                && valid_url(s)
                            {
                                out.push(s.to_string())
                            }
                        }
                    };
                    for x in m.values() {
                        urls(x, out)
                    }
                }
                _ => {}
            }
        }
        let mut links = Vec::new();
        urls(&self.fields["description"], &mut links);
        for found in URLS.find_iter(&self.description()) {
            let url = found.as_str().trim_end_matches(['.', ',', ';', ')', ']']);
            if valid_url(url) {
                links.push(url.to_string())
            }
        }
        for url in links {
            add(self.url_target(&url, &url));
        }
        for a in self.attachments() {
            if let Some(url) = self.web_url() {
                add(Target {
                    label: format!("{} · open in Jira", a.name),
                    url: Some(url),
                    attachment: Some(a),
                    key: None,
                });
            }
        }
        if let Some(url) = self.web_url() {
            add(Target {
                label: "Open ticket in Jira".into(),
                url: Some(url),
                ..Default::default()
            });
        }
        out
    }
    pub fn url_target(&self, label: &str, url: &str) -> Target {
        if let (Ok(u), Ok(site)) = (Url::parse(url), Url::parse(&self.self_url))
            && u.host_str() == site.host_str()
            && u.port_or_known_default() == site.port_or_known_default()
            && let Some(key) = u.path().strip_prefix("/browse/")
        {
            let key = key.trim_end_matches('/');
            if valid_key(key) {
                return Target::ticket(label, key);
            }
        }
        Target {
            label: safe(label),
            url: valid_url(url).then(|| url.to_string()),
            ..Default::default()
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct User {
    pub name: String,
    pub email: String,
    pub id: String,
    pub login: String,
}
impl User {
    pub fn from_value(v: &Value) -> Self {
        Self {
            name: text(&v["displayName"]),
            email: text(&v["emailAddress"]),
            id: text(&v["accountId"]),
            login: text(&v["name"]),
        }
    }
    pub fn label(&self) -> String {
        if !self.name.is_empty() {
            self.name.clone()
        } else if !self.email.is_empty() {
            self.email.clone()
        } else {
            "Unassigned".into()
        }
    }
    /// Ticket fields are blank when no person is present. Selector/action labels
    /// keep their explicit Unassigned option; opaque assigned users are not null.
    pub fn display_label(&self) -> String {
        [&self.name, &self.email, &self.login, &self.id]
            .into_iter()
            .find(|s| !s.is_empty())
            .cloned()
            .unwrap_or_default()
    }
    pub fn identity(&self, jira: bool) -> String {
        if jira {
            if !self.email.is_empty() {
                self.email.clone()
            } else {
                self.login.clone()
            }
        } else if !self.id.is_empty() {
            self.id.clone()
        } else {
            self.email.clone()
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Attachment {
    pub id: String,
    pub name: String,
    pub mime: String,
    #[serde(default)]
    pub size: u64,
}
impl Attachment {
    pub fn is_image(&self) -> bool {
        matches!(
            self.mime.to_ascii_lowercase().as_str(),
            "image/png" | "image/jpeg" | "image/jpg" | "image/gif" | "image/webp"
        )
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Target {
    pub label: String,
    pub key: Option<String>,
    pub url: Option<String>,
    pub attachment: Option<Attachment>,
}
impl Target {
    pub fn ticket(label: impl Into<String>, key: &str) -> Self {
        Self {
            label: label.into(),
            key: Some(key.to_uppercase()),
            ..Default::default()
        }
    }
}
#[derive(Clone, Debug, Default)]
pub struct Board {
    pub id: u64,
    pub name: String,
    pub kind: String,
}

pub fn text(v: &Value) -> String {
    match v {
        Value::String(s) => safe(s),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}
pub fn safe(s: &str) -> String {
    // Remove CSI/OSC/DCS controls as well as stray controls from untrusted Jira text.
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            match chars.next() {
                Some('[') => {
                    for x in chars.by_ref() {
                        if ('@'..='~').contains(&x) {
                            break;
                        }
                    }
                }
                Some(']') | Some('P') | Some('_') => {
                    while let Some(x) = chars.next() {
                        if x == '\x07' {
                            break;
                        }
                        if x == '\x1b' && chars.peek() == Some(&'\\') {
                            chars.next();
                            break;
                        }
                    }
                }
                _ => {}
            };
            continue;
        }
        if !c.is_control() || matches!(c, '\n' | '\t') {
            out.push(c)
        }
    }
    out
}
pub fn line(s: &str) -> String {
    safe(s).split_whitespace().collect::<Vec<_>>().join(" ")
}
pub fn valid_key(key: &str) -> bool {
    KEY.is_match(key)
}
pub fn valid_url(url: &str) -> bool {
    if url.chars().any(char::is_control) {
        return false;
    }
    Url::parse(url).is_ok_and(|u| {
        matches!(u.scheme(), "http" | "https")
            && u.host_str().is_some()
            && u.username().is_empty()
            && u.password().is_none()
    })
}
pub fn require_key(key: &str) -> Result<()> {
    if !valid_key(key) {
        bail!("Invalid ticket key")
    };
    Ok(())
}
pub fn number_key(prefix: &str, digits: &str) -> Option<String> {
    if prefix.is_empty() || digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let key = format!("{}-{digits}", prefix.to_uppercase());
    valid_key(&key).then_some(key)
}
fn escape_md(s: &str) -> String {
    safe(s)
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace('*', "\\*")
        .replace('_', "\\_")
        .replace('`', "\\`")
        .replace('~', "\\~")
}
pub fn adf_markdown(v: &Value) -> String {
    if let Some(s) = v.as_str() {
        return safe(s);
    }
    let kind = v["type"].as_str().unwrap_or("");
    let attrs = &v["attrs"];
    let body = v["content"]
        .as_array()
        .map(|a| a.iter().map(adf_markdown).collect::<String>())
        .unwrap_or_default();
    match kind {
        "text" => {
            let code = v["marks"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|m| m["type"] == "code");
            let mut s = if code {
                safe(v["text"].as_str().unwrap_or(""))
            } else {
                escape_md(v["text"].as_str().unwrap_or(""))
            };
            for mark in v["marks"].as_array().into_iter().flatten() {
                match mark["type"].as_str().unwrap_or("") {
                    "strong" => s = format!("**{s}**"),
                    "em" => s = format!("_{s}_"),
                    "strike" => s = format!("~~{s}~~"),
                    "code" => s = format!("`{s}`"),
                    "link" => {
                        let u = text(&mark["attrs"]["href"]);
                        if valid_url(&u) {
                            s = format!("[{s}]({})", u.replace(')', "%29"))
                        }
                    }
                    _ => {}
                }
            }
            s
        }
        "paragraph" => format!("{body}\n\n"),
        "heading" => format!(
            "{} {body}\n\n",
            "#".repeat(attrs["level"].as_u64().unwrap_or(2).clamp(1, 6) as usize)
        ),
        "hardBreak" => "\n".into(),
        "mention" => text(&attrs["text"]),
        "emoji" => {
            let s = text(&attrs["text"]);
            if s.is_empty() {
                text(&attrs["shortName"])
            } else {
                s
            }
        }
        "codeBlock" => {
            fn plain(v: &Value) -> String {
                if v["type"] == "text" {
                    text(&v["text"])
                } else {
                    v["content"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(plain)
                        .collect()
                }
            }
            let body = plain(v);
            let fence = if body.contains("```") { "````" } else { "```" };
            format!(
                "{fence}{}\n{body}\n{fence}\n\n",
                line(&text(&attrs["language"]))
            )
        }
        "blockquote" => body.lines().map(|s| format!("> {s}\n")).collect::<String>() + "\n",
        "listItem" => format!("- {}\n", body.trim().replace('\n', "\n  ")),
        "bulletList" => body + "\n",
        "orderedList" => {
            let start = attrs["order"].as_u64().unwrap_or(1);
            v["content"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
                .map(|(n, item)| {
                    format!(
                        "{}. {}",
                        start + n as u64,
                        adf_markdown(item).trim_start_matches("- ")
                    )
                })
                .collect::<String>()
                + "\n"
        }
        "rule" => "---\n\n".into(),
        "inlineCard" | "blockCard" => {
            let u = text(&attrs["url"]);
            if valid_url(&u) {
                format!("[{u}]({u})")
            } else {
                String::new()
            }
        }
        "media" => "[image/attachment — see Attachments below]".into(),
        "mediaSingle" | "mediaGroup" => body + "\n\n",
        // GFM cells must stay on one source line. ADF paragraphs/hard breaks
        // otherwise split a table row into ordinary Markdown paragraphs.
        "tableCell" | "tableHeader" => {
            body.lines()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("<br>")
                .replace('|', "\\|")
                + " | "
        }
        "tableRow" => format!("| {body}\n"),
        "table" => {
            let rows = v["content"].as_array().cloned().unwrap_or_default();
            let count = rows
                .first()
                .and_then(|r| r["content"].as_array())
                .map(Vec::len)
                .unwrap_or(0);
            rows.iter()
                .enumerate()
                .map(|(n, r)| {
                    let mut row = adf_markdown(r);
                    if n == 0 {
                        row.push_str(&format!("|{}\n", " --- |".repeat(count)))
                    };
                    row
                })
                .collect::<String>()
                + "\n"
        }
        _ => body,
    }
}
pub fn demo_tickets() -> Vec<Ticket> {
    serde_json::from_value(json!([
        {"key":"DEV-101","estimate_fields":["customfield_90123"],"self":"https://example.atlassian.net/rest/api/3/issue/101","fields":{"customfield_90123":3,"summary":"A simpler two-pane terminal workspace","status":{"name":"In Progress","statusCategory":{"key":"indeterminate"}},"assignee":{"displayName":"Alex","accountId":"alex","emailAddress":"alex@example.com"},"reporter":{"displayName":"Sam"},"priority":{"name":"High"},"issuetype":{"name":"Story"},"description":{"type":"doc","content":[{"type":"heading","attrs":{"level":2},"content":[{"type":"text","text":"Fast navigation"}]},{"type":"paragraph","content":[{"type":"text","text":"Bold","marks":[{"type":"strong"}]},{"type":"text","text":" descriptions with "},{"type":"text","text":"links","marks":[{"type":"link","attrs":{"href":"https://example.com/docs"}}]}]}]},"issuelinks":[{"type":{"outward":"blocks"},"outwardIssue":{"key":"DEV-102","fields":{"summary":"Improve number lookup"}}}],"comment":{"total":1,"comments":[{"author":{"displayName":"Sam"},"body":"Try **101** to find this ticket."}]}}},
        {"key":"DEV-102","estimate_fields":["customfield_90123"],"self":"https://example.atlassian.net/rest/api/3/issue/102","fields":{"customfield_90123":0,"summary":"Typing digits finds a ticket automatically","status":{"name":"Refinement","statusCategory":{"key":"new"}},"assignee":null,"reporter":{"displayName":"Alex"},"priority":{"name":"Medium"},"issuetype":{"name":"Task"},"description":"# Ticket search\n\nType `102` anywhere in the main view.\n\n- No Enter required\n- Ctrl+1 switches views instead\n\nRelated: DEV-101"}},
        {"key":"DEV-103","self":"https://example.atlassian.net/rest/api/3/issue/103","fields":{"summary":"Make assignment explicit and safe","status":{"name":"To Do","statusCategory":{"key":"new"}},"assignee":{"displayName":"Sam","accountId":"sam","emailAddress":"sam@example.com"},"reporter":{"displayName":"Alex"},"priority":{"name":"Low"},"issuetype":{"name":"Task"},"description":"Choose **Me**, **Unassigned**, a known person, or enter an email/account ID.\n\nUpdates need a separate confirmation."}}
    ])).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn absent_people_display_as_blank_without_mistaking_opaque_users_for_unassigned() {
        assert_eq!(User::default().display_label(), "");
        assert_eq!(User::default().label(), "Unassigned");
        assert_eq!(
            User {
                id: "opaque-id".into(),
                ..Default::default()
            }
            .display_label(),
            "opaque-id"
        );
        assert_eq!(
            User {
                name: "Sam".into(),
                ..Default::default()
            }
            .display_label(),
            "Sam"
        );
        let mut t = demo_tickets().remove(0);
        t.key = "DEV-1\u{1b}]52;bad".into();
        assert!(t.web_url().is_none());
    }
    #[test]
    fn story_points_keep_zero_and_fractions_and_never_guess_or_sum_fields() {
        let mut t = demo_tickets().remove(0);
        assert_eq!(t.points_label(), "3");
        for (value, expected) in [
            (json!(0), "0"),
            (json!(2.5), "2.5"),
            (Value::Null, "—"),
            (json!(-1), "—"),
            (json!("5"), "—"),
            (json!(true), "—"),
        ] {
            t.fields["customfield_90123"] = value;
            assert_eq!(t.points_label(), expected);
        }
        t.estimate_fields.clear();
        t.fields["customfield_90123"] = json!(5);
        assert_eq!(t.points_label(), "—");
        t.names = json!({"customfield_90123":"Story Points"});
        assert_eq!(t.points_label(), "5");
        t.estimate_fields = vec!["customfield_90234".into()];
        t.fields["customfield_90234"] = json!(8);
        assert_eq!(t.points_label(), "—");
        t.fields["customfield_90234"] = Value::Null;
        assert_eq!(t.points_label(), "5");
        let mut row = Ticket {
            fields: json!({}),
            ..Default::default()
        };
        row.update_estimates(&t);
        assert_eq!(row.points_label(), "5");
    }
    #[test]
    fn digits_and_keys() {
        assert_eq!(number_key("DEV", "3084").as_deref(), Some("DEV-3084"));
        assert!(number_key("DEV", "1;rm").is_none());
        assert!(!valid_key("DEV-1,DEV-2"));
    }
    #[test]
    fn list_inventory_includes_epics_but_excludes_child_tasks() {
        let mut ticket = demo_tickets().remove(0);
        assert!(ticket.backlog_ticket());
        for kind in ["Sub-Task", "Subtask"] {
            ticket.fields["issuetype"] = json!({"name":kind});
            assert!(!ticket.backlog_ticket());
        }
        ticket.fields["issuetype"] = json!({"name":"Localized child type","subtask":true});
        assert!(!ticket.backlog_ticket());
        for kind in ["Epic", "Bug", "Task", "Story", "Design"] {
            ticket.fields["issuetype"] = json!({"name":kind});
            assert!(ticket.backlog_ticket());
        }
    }
    #[test]
    fn markdown_marks() {
        let v = json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"bold","marks":[{"type":"strong"}]},{"type":"text","text":" code","marks":[{"type":"code"}]}]}]});
        let md = adf_markdown(&v);
        assert!(md.contains("**bold**"));
        assert!(md.contains("` code`"));
    }
    #[test]
    fn literal_markdown_code_and_ordered_lists() {
        assert_eq!(
            adf_markdown(&json!({"type":"text","text":"a *literal* _word_"})),
            "a \\*literal\\* \\_word\\_"
        );
        let code = json!({"type":"codeBlock","content":[{"type":"text","text":"x[0] = a_b;"}]});
        assert!(adf_markdown(&code).contains("x[0] = a_b;"));
        let list = json!({"type":"orderedList","attrs":{"order":3},"content":[{"type":"listItem","content":[{"type":"paragraph","content":[{"type":"text","text":"item"}]}]}]});
        assert!(adf_markdown(&list).starts_with("3. item"));
    }
    #[test]
    fn nullable_cli_fields() {
        let t: Ticket = serde_json::from_value(
            json!({"key":"DEV-1","self":null,"transitions":null,"names":null}),
        )
        .unwrap();
        assert!(t.transitions.is_empty());
    }
    #[test]
    fn urls_and_controls() {
        assert!(!valid_url("javascript:alert(1)"));
        assert!(!valid_url("https://user:pass@example.com"));
        assert_eq!(safe("a\x1b[31mb\x1b[0m\x1b]52;c;secret\x07"), "ab");
        let i = demo_tickets().remove(0);
        assert_eq!(
            i.url_target("related", "https://example.atlassian.net/browse/DEV-102")
                .key
                .as_deref(),
            Some("DEV-102")
        );
    }
}
