use crate::{
    backend::{Action, Backend, Kind},
    config::{self, Cache, Settings},
    data::{self, Target, Ticket},
    markdown::{self, DocLine},
};
use anyhow::Result;
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use ratatui::{layout::Rect, widgets::TableState};
use ratatui_image::protocol::Protocol;
use std::{
    collections::{BTreeMap, HashSet},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

pub enum Job {
    List {
        generation: u64,
        query: String,
        limit: usize,
    },
    Detail {
        key: String,
        lookup: Option<u64>,
    },
    Comments(String),
    Write(Action),
    Browser(String),
    Preview {
        generation: u64,
        path: PathBuf,
        size: (u16, u16),
    },
}
pub enum Message {
    List {
        generation: u64,
        query: String,
        result: Result<Vec<Ticket>>,
    },
    Detail {
        key: String,
        lookup: Option<u64>,
        result: Result<Ticket>,
    },
    Comments {
        key: String,
        result: Result<Vec<serde_json::Value>>,
    },
    Written {
        action: Action,
        result: Result<()>,
    },
    Notice(Result<String>),
    Image {
        generation: u64,
        name: String,
        result: Result<Protocol>,
    },
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Tickets,
    Ticket,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    None,
    Text,
    Number,
}
#[derive(Clone)]
pub struct Choice {
    pub label: String,
    pub value: String,
    pub target: Option<Target>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    Assign,
    Status,
    Links,
    Edit,
    AssigneeFilter,
    StatusFilter,
}
pub enum Dialog {
    Picker {
        title: String,
        choices: Vec<Choice>,
        selected: usize,
        purpose: Purpose,
        key: String,
    },
    Form {
        title: String,
        labels: Vec<String>,
        values: Vec<String>,
        selected: usize,
        kind: String,
        key: String,
    },
    Editor {
        title: String,
        value: String,
        cursor: usize,
        kind: String,
        key: String,
    },
    Confirm(Action),
    Info {
        title: String,
        text: String,
        scroll: u16,
    },
}
pub struct App {
    pub backend: Arc<Backend>,
    pub cache: Cache,
    pub settings: Settings,
    pub read_only: bool,
    pub limit: usize,
    pub view: usize,
    pub items: Vec<Ticket>,
    pub visible: Vec<usize>,
    pub selected: usize,
    pub table: TableState,
    pub focus: Focus,
    pub details: BTreeMap<String, (Ticket, u64)>,
    pub detail_key: String,
    pub document: Vec<DocLine>,
    pub doc_width: u16,
    pub doc_dirty: bool,
    pub scroll: usize,
    pub history: Vec<(String, usize)>,
    pub search_mode: SearchMode,
    pub search: String,
    pub remote_search: String,
    pub local_search: String,
    pub assignee_filter: String,
    pub status_filter: String,
    pub search_due: Option<Instant>,
    pub selection_due: Option<Instant>,
    pub generation: u64,
    pub lookup_generation: u64,
    pub lookup_active: bool,
    pub last_query: String,
    pub pending: HashSet<String>,
    pub loading: bool,
    pub writing: bool,
    pub dialog: Option<Dialog>,
    pub status: String,
    pub error: String,
    pub quit: bool,
    pub preview: Option<(Protocol, String)>,
    pub image_generation: u64,
    pub awaiting_image: bool,
    pub attachment_dir: PathBuf,
    pub tabs: Vec<(Rect, usize)>,
    pub scope_rect: Rect,
    pub search_rect: Rect,
    pub assignee_rect: Rect,
    pub status_rect: Rect,
    pub tickets_rect: Rect,
    pub ticket_rect: Rect,
    pub popup_rect: Rect,
    pub picker_start: usize,
    pub row_height: u16,
    pub footer_buttons: Vec<(Rect, char)>,
}
impl App {
    pub fn new(
        backend: Arc<Backend>,
        cache: Cache,
        settings: Settings,
        read_only: bool,
        limit: usize,
        attachment_dir: PathBuf,
    ) -> Self {
        let view = settings.last_view.min(config::BACKLOG);
        Self {
            backend,
            cache,
            settings,
            read_only,
            limit,
            view,
            items: vec![],
            visible: vec![],
            selected: 0,
            table: TableState::default(),
            focus: Focus::Tickets,
            details: BTreeMap::new(),
            detail_key: String::new(),
            document: vec![],
            doc_width: 0,
            doc_dirty: true,
            scroll: 0,
            history: vec![],
            search_mode: SearchMode::None,
            search: String::new(),
            remote_search: String::new(),
            local_search: String::new(),
            assignee_filter: String::new(),
            status_filter: String::new(),
            search_due: None,
            selection_due: None,
            generation: 0,
            lookup_generation: 0,
            lookup_active: false,
            last_query: String::new(),
            pending: HashSet::new(),
            loading: false,
            writing: false,
            dialog: None,
            status: "Ready".into(),
            error: String::new(),
            quit: false,
            preview: None,
            image_generation: 0,
            awaiting_image: false,
            attachment_dir,
            tabs: vec![],
            scope_rect: Rect::default(),
            search_rect: Rect::default(),
            assignee_rect: Rect::default(),
            status_rect: Rect::default(),
            tickets_rect: Rect::default(),
            ticket_rect: Rect::default(),
            popup_rect: Rect::default(),
            picker_start: 0,
            row_height: 1,
            footer_buttons: vec![],
        }
    }
    pub fn notice(&mut self, s: impl Into<String>, bad: bool) {
        let s = data::safe(&s.into());
        self.status = data::line(&s);
        if bad {
            self.error = s
        }
    }
    pub fn view_names(&self) -> Vec<String> {
        config::VIEW_NAMES.iter().map(|s| s.to_string()).collect()
    }
    pub fn query(&self) -> Result<String> {
        self.settings.query(
            self.view,
            &self.remote_search,
            &self.assignee_filter,
            &self.status_filter,
        )
    }
    pub fn reload(&mut self) -> Vec<Job> {
        match self.query() {
            Ok(q) => self.load(q),
            Err(e) => {
                self.generation += 1;
                self.loading = false;
                self.items.clear();
                self.visible.clear();
                self.table.select(None);
                self.detail_key.clear();
                self.doc_dirty = true;
                self.notice(e.to_string(), true);
                if self.settings.project.is_empty() && self.settings.scope_jql.is_empty() {
                    self.scope_dialog()
                };
                vec![]
            }
        }
    }
    fn load(&mut self, query: String) -> Vec<Job> {
        let same_query = query == self.last_query;
        self.generation += 1;
        self.last_query = query.clone();
        self.loading = true;
        self.notice("Loading…", false);
        if let Some((items, _)) = self
            .cache
            .get::<Vec<Ticket>>(&format!("list:{}:{query}", self.limit))
        {
            self.set_items(items);
            self.notice("Cached tickets · refreshing", false)
        } else if !same_query {
            self.items.clear();
            self.selected = 0;
            self.rebuild_visible();
            if self.history.is_empty() {
                self.detail_key.clear();
                self.doc_dirty = true
            }
        }
        vec![Job::List {
            generation: self.generation,
            query,
            limit: self.limit,
        }]
    }
    fn set_items(&mut self, items: Vec<Ticket>) {
        let key = self.selected_ticket().map(|t| t.key.clone());
        self.items = items;
        self.local_search.clear();
        self.rebuild_visible();
        if let Some(key) = key
            && let Some(n) = self.visible.iter().position(|n| self.items[*n].key == key)
        {
            self.selected = n
        }
        if self.focus != Focus::Ticket || self.history.is_empty() {
            self.select_current();
        }
    }
    pub fn selected_ticket(&self) -> Option<&Ticket> {
        self.visible
            .get(self.selected)
            .and_then(|n| self.items.get(*n))
    }
    pub fn active_ticket(&self) -> Option<&Ticket> {
        if self.focus == Focus::Ticket {
            self.details
                .get(&self.detail_key)
                .map(|x| &x.0)
                .or_else(|| self.selected_ticket())
        } else {
            self.selected_ticket()
        }
    }
    fn rebuild_visible(&mut self) {
        let q = self.local_search.to_lowercase();
        self.visible = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(n, t)| {
                let matches = if self.search_mode == SearchMode::Number {
                    t.key
                        .to_lowercase()
                        .starts_with(&format!("{}-{}", self.settings.prefix(), q).to_lowercase())
                } else {
                    q.is_empty()
                        || format!(
                            "{} {} {} {} {}",
                            t.key,
                            t.summary(),
                            t.status(),
                            t.user("assignee").label(),
                            t.user("reporter").label()
                        )
                        .to_lowercase()
                        .contains(&q)
                };
                matches.then_some(n)
            })
            .collect();
        self.selected = self.selected.min(self.visible.len().saturating_sub(1));
        self.table
            .select((!self.visible.is_empty()).then_some(self.selected));
    }
    fn select_current(&mut self) {
        if let Some(key) = self.selected_ticket().map(|t| t.key.clone()) {
            self.detail_key = key;
            self.history.clear();
            self.scroll = 0;
            self.doc_dirty = true;
            self.selection_due = Some(Instant::now() + Duration::from_millis(160));
        } else {
            self.detail_key.clear();
            self.doc_dirty = true
        }
    }
    fn detail_job(&mut self, key: String, lookup: Option<u64>, force: bool) -> Vec<Job> {
        if self.pending.contains(&key) && lookup.is_none() {
            return vec![];
        };
        if lookup.is_none() && !force {
            if let Some((_, time)) = self.details.get(&key)
                && config::now().saturating_sub(*time) < 120
            {
                return vec![];
            }
            if let Some((ticket, time)) = self.cache.get::<Ticket>(&format!("detail:{key}")) {
                self.details.insert(key.clone(), (ticket, time));
                self.doc_dirty = true;
                if config::now().saturating_sub(time) < 120 {
                    return vec![];
                }
            }
        }
        self.pending.insert(key.clone());
        vec![Job::Detail { key, lookup }]
    }
    pub fn advance(&mut self) -> Vec<Job> {
        let now = Instant::now();
        if self.search_due.is_some_and(|d| now >= d) {
            self.search_due = None;
            match self.search_mode {
                SearchMode::Number => {
                    if let Some(key) = data::number_key(self.settings.prefix(), &self.search) {
                        self.loading = true;
                        return self.detail_job(key, Some(self.lookup_generation), true);
                    }
                }
                SearchMode::Text => {
                    self.remote_search = self.search.clone();
                    return self.reload();
                }
                _ => {}
            }
        }
        if self.selection_due.is_some_and(|d| now >= d) {
            self.selection_due = None;
            if !self.detail_key.is_empty() {
                return self.detail_job(self.detail_key.clone(), None, false);
            }
        }
        vec![]
    }
    pub fn message(&mut self, message: Message) -> Vec<Job> {
        match message {
            Message::List {
                generation,
                query,
                result,
            } => {
                if generation != self.generation {
                    return vec![];
                };
                self.loading = false;
                match result {
                    Ok(items) => {
                        let count = items.len();
                        let _ = self
                            .cache
                            .put(&format!("list:{}:{query}", self.limit), &items);
                        self.set_items(items);
                        self.notice(
                            format!(
                                "{count} tickets{}",
                                if count == self.limit {
                                    " · limit reached; narrow the query"
                                } else {
                                    ""
                                }
                            ),
                            false,
                        )
                    }
                    Err(e) => self.notice(e.to_string(), true),
                }
            }
            Message::Detail {
                key,
                lookup,
                result,
            } => {
                self.pending.remove(&key);
                if lookup.is_some() && lookup != Some(self.lookup_generation) {
                    return vec![];
                };
                if lookup.is_some() {
                    self.loading = false
                };
                match result {
                    Ok(ticket) => {
                        let _ = self.cache.put(&format!("detail:{key}"), &ticket);
                        self.details
                            .insert(key.clone(), (ticket.clone(), config::now()));
                        for item in &mut self.items {
                            if item.key == key {
                                for field in [
                                    "summary",
                                    "status",
                                    "assignee",
                                    "reporter",
                                    "priority",
                                    "issuetype",
                                    "labels",
                                ] {
                                    item.fields[field] = ticket.fields[field].clone();
                                }
                            }
                        }
                        while self.details.len() > 64 {
                            let old = self
                                .details
                                .iter()
                                .filter(|(k, _)| *k != &key && *k != &self.detail_key)
                                .min_by_key(|(_, (_, time))| *time)
                                .map(|(k, _)| k.clone());
                            if let Some(old) = old {
                                self.details.remove(&old);
                            } else {
                                break;
                            }
                        }
                        if lookup.is_some() {
                            self.items = vec![ticket];
                            self.local_search.clear();
                            self.visible = vec![0];
                            self.selected = 0;
                            self.table.select(Some(0));
                            self.detail_key = key.clone();
                            self.scroll = 0;
                            self.notice(
                                format!("Found {key} · a assigns · Esc restores the list"),
                                false,
                            )
                        }
                        if self.detail_key == key {
                            self.doc_dirty = true
                        }
                    }
                    Err(e) => {
                        if lookup.is_some() || self.detail_key == key {
                            self.notice(format!("{key}: {e}"), true)
                        }
                    }
                }
            }
            Message::Comments { key, result } => {
                self.pending.remove(&format!("comments:{key}"));
                match result {
                    Ok(comments) => {
                        if let Some((ticket, time)) = self.details.get_mut(&key) {
                            ticket.fields["comment"] =
                                serde_json::json!({"total":comments.len(),"comments":comments});
                            *time = config::now();
                            let _ = self.cache.put(&format!("detail:{key}"), ticket);
                            self.doc_dirty = true
                        }
                        self.notice("All comments loaded", false)
                    }
                    Err(e) => self.notice(e.to_string(), true),
                }
            }
            Message::Written { action, result } => {
                self.writing = false;
                self.details.remove(&action.key);
                self.doc_dirty = true;
                self.cache.remove(&format!("detail:{}", action.key));
                self.cache
                    .remove(&format!("list:{}:{}", self.limit, self.last_query));
                match result {
                    Ok(()) => {
                        self.notice(format!("Updated {}", action.key), false);
                        let mut jobs = if self.lookup_active {
                            vec![]
                        } else {
                            self.reload()
                        };
                        jobs.extend(self.detail_job(
                            action.key,
                            if self.lookup_active && self.history.is_empty() {
                                Some(self.lookup_generation)
                            } else {
                                None
                            },
                            true,
                        ));
                        return jobs;
                    }
                    Err(e) => {
                        self.notice(
                            format!(
                                "Update failed or outcome unknown: {e}\nReload before retrying."
                            ),
                            true,
                        );
                        self.dialog = Some(Dialog::Info {
                            title: "Update did not complete successfully".into(),
                            text: self.error.clone(),
                            scroll: 0,
                        });
                        return self.detail_job(action.key, None, true);
                    }
                }
            }
            Message::Notice(result) => match result {
                Ok(text) => self.notice(text, false),
                Err(e) => self.notice(e.to_string(), true),
            },
            Message::Image {
                generation,
                name,
                result,
            } => {
                if generation == self.image_generation && self.awaiting_image {
                    self.awaiting_image = false;
                    match result {
                        Ok(image) => {
                            self.preview = Some((image, name));
                            self.notice("Image preview · Esc closes", false)
                        }
                        Err(e) => self.notice(e.to_string(), true),
                    }
                }
            }
        }
        vec![]
    }
    pub fn document(&mut self, width: u16) {
        if self.doc_dirty || self.doc_width != width {
            self.document = if let Some((ticket, _)) = self.details.get(&self.detail_key) {
                markdown::ticket_document(ticket, width)
            } else {
                vec![]
            };
            self.doc_width = width;
            self.doc_dirty = false;
            self.scroll = self.scroll.min(self.document.len().saturating_sub(1));
        }
    }
    fn switch_view(&mut self, index: usize) -> Vec<Job> {
        if index >= self.view_names().len() {
            return vec![];
        };
        self.view = index;
        self.assignee_filter.clear();
        self.settings.last_view = index;
        if self.backend.kind != Kind::Demo {
            let _ = self.settings.save();
        };
        if self.search_mode == SearchMode::Text {
            self.remote_search = self.search.clone();
        }
        self.search_mode = SearchMode::None;
        self.lookup_active = false;
        self.search = self.remote_search.clone();
        self.local_search.clear();
        self.search_due = None;
        self.lookup_generation += 1;
        self.focus = Focus::Tickets;
        self.history.clear();
        self.reload()
    }
    fn search_changed(&mut self) {
        self.generation += 1;
        self.lookup_generation += 1;
        self.loading = false;
        self.local_search = self.search.clone();
        self.selected = 0;
        self.rebuild_visible();
        self.search_due = Some(Instant::now() + Duration::from_millis(300));
    }
    pub fn event(&mut self, event: Event) -> Vec<Job> {
        match event {
            Event::Paste(text) => {
                let text = data::safe(&text);
                match &mut self.dialog {
                    Some(Dialog::Editor { value, cursor, .. }) => {
                        crate::input::insert(value, cursor, &text)
                    }
                    Some(Dialog::Form {
                        values, selected, ..
                    }) => values[*selected].push_str(&data::line(&text)),
                    Some(_) => {}
                    None => {
                        let text = data::line(&text);
                        let digits = text
                            .strip_prefix(&format!("{}-", self.settings.prefix()))
                            .unwrap_or(&text);
                        if digits.is_empty() {
                            return vec![];
                        }
                        if digits.bytes().all(|b| b.is_ascii_digit()) {
                            self.lookup_active = true;
                            self.search_mode = SearchMode::Number;
                            self.search = digits.to_string();
                        } else {
                            self.lookup_active = false;
                            self.search_mode = SearchMode::Text;
                            self.search = text;
                        }
                        self.search_changed();
                    }
                }
            }
            Event::Resize(..) => {
                self.doc_dirty = true;
                self.preview = None;
                self.image_generation += 1;
                self.awaiting_image = false
            }
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                if self.preview.is_some() || self.awaiting_image {
                    if matches!(key.code, KeyCode::Esc | KeyCode::Char('q')) {
                        self.preview = None;
                        self.awaiting_image = false;
                        self.image_generation += 1
                    };
                    return vec![];
                }
                if self.dialog.is_some() {
                    return self.dialog_key(key);
                }
                if let KeyCode::Char(c @ '1'..='9') = key.code
                    && key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                {
                    return self.switch_view(c as usize - '1' as usize);
                }
                if let KeyCode::F(n) = key.code
                    && (1..=9).contains(&n)
                {
                    return self.switch_view(n as usize - 1);
                }
                if self.search_mode != SearchMode::None {
                    return self.search_key(key);
                }
                if let KeyCode::Char(c @ '0'..='9') = key.code {
                    self.search_mode = SearchMode::Number;
                    self.lookup_active = true;
                    self.focus = Focus::Tickets;
                    self.history.clear();
                    self.search = c.to_string();
                    self.search_changed();
                    return vec![];
                }
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('g') {
                    self.scope_dialog();
                    return vec![];
                }
                match key.code {
                    KeyCode::Char('q') => {
                        if self.writing {
                            self.notice("Wait for the in-flight update before quitting", true)
                        } else {
                            self.quit = true
                        }
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        if !self.writing {
                            self.quit = true
                        }
                    }
                    KeyCode::Tab | KeyCode::BackTab => {
                        self.focus = if self.focus == Focus::Tickets {
                            Focus::Ticket
                        } else {
                            Focus::Tickets
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if self.focus == Focus::Ticket {
                            self.scroll =
                                (self.scroll + 1).min(self.document.len().saturating_sub(1))
                        } else {
                            self.selected =
                                (self.selected + 1).min(self.visible.len().saturating_sub(1));
                            self.table.select(Some(self.selected));
                            self.select_current()
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        if self.focus == Focus::Ticket {
                            self.scroll = self.scroll.saturating_sub(1)
                        } else {
                            self.selected = self.selected.saturating_sub(1);
                            self.table.select(Some(self.selected));
                            self.select_current()
                        }
                    }
                    KeyCode::PageDown => {
                        if self.focus == Focus::Ticket {
                            self.scroll =
                                (self.scroll + 10).min(self.document.len().saturating_sub(1))
                        } else {
                            self.selected =
                                (self.selected + 10).min(self.visible.len().saturating_sub(1));
                            self.table.select(Some(self.selected));
                            self.select_current()
                        }
                    }
                    KeyCode::PageUp => {
                        if self.focus == Focus::Ticket {
                            self.scroll = self.scroll.saturating_sub(10)
                        } else {
                            self.selected = self.selected.saturating_sub(10);
                            self.table.select(Some(self.selected));
                            self.select_current()
                        }
                    }
                    KeyCode::Enter => {
                        self.focus = Focus::Ticket;
                        return self.detail_job(self.detail_key.clone(), None, false);
                    }
                    KeyCode::Esc | KeyCode::Backspace => {
                        if let Some((key, scroll)) = self.history.pop() {
                            self.detail_key = key.clone();
                            self.scroll = scroll;
                            self.doc_dirty = true;
                            return self.detail_job(key, None, false);
                        }
                        self.focus = Focus::Tickets;
                        if self.lookup_active || !self.remote_search.is_empty() {
                            self.lookup_active = false;
                            self.search.clear();
                            self.remote_search.clear();
                            self.lookup_generation += 1;
                            return self.reload();
                        }
                    }
                    KeyCode::Char('/') => {
                        self.search_mode = SearchMode::Text;
                        self.search.clear();
                        self.local_search.clear()
                    }
                    KeyCode::Char('g') => self.scope_dialog(),
                    KeyCode::Char('A') => self.assignee_picker(true),
                    KeyCode::Char('T') => self.status_picker(true),
                    KeyCode::Char('f') => self.filters_dialog(),
                    KeyCode::Char(':') => {
                        self.dialog = Some(Dialog::Form {
                            title: "Scope JQL · no ORDER BY in this field".into(),
                            labels: vec!["Scope JQL".into()],
                            values: vec![self.settings.scope_jql.clone()],
                            selected: 0,
                            kind: "jql".into(),
                            key: String::new(),
                        })
                    }
                    KeyCode::Char('r') => {
                        let key = self.detail_key.clone();
                        let mut jobs = if self.lookup_active {
                            vec![]
                        } else {
                            self.reload()
                        };
                        if !key.is_empty() {
                            jobs.extend(self.detail_job(key, None, true))
                        };
                        return jobs;
                    }
                    KeyCode::Char('l') => self.links(),
                    KeyCode::Char('O') => {
                        if let Some(url) = self.active_ticket().and_then(Ticket::web_url) {
                            return vec![Job::Browser(url)];
                        }
                    }
                    KeyCode::Char('C') => {
                        let key = self.detail_key.clone();
                        if !key.is_empty() && self.pending.insert(format!("comments:{key}")) {
                            return vec![Job::Comments(key)];
                        }
                    }
                    KeyCode::Char('a' | 't' | 'e' | 'c' | 'L') => {
                        if let KeyCode::Char(c) = key.code {
                            self.action_dialog(c)
                        }
                    }
                    KeyCode::Char('i') => {
                        self.dialog = Some(Dialog::Form {
                            title: "Preview a downloaded image".into(),
                            labels: vec!["Local image path".into()],
                            values: vec![String::new()],
                            selected: 0,
                            kind: "image".into(),
                            key: String::new(),
                        })
                    }
                    KeyCode::Char('!') => {
                        self.dialog = Some(Dialog::Info {
                            title: "Last CLI error".into(),
                            text: self.error.clone(),
                            scroll: 0,
                        })
                    }
                    KeyCode::Char('?') => {
                        self.dialog = Some(Dialog::Info {
                            title: "Simple Jira workspace".into(),
                            text: HELP.into(),
                            scroll: 0,
                        })
                    }
                    _ => {}
                }
            }
            Event::Mouse(_) if self.preview.is_some() || self.awaiting_image => {}
            Event::Mouse(mouse) => {
                if self.dialog.is_some() {
                    if mouse.kind == MouseEventKind::Down(MouseButton::Left)
                        && let Some(Dialog::Picker {
                            choices, selected, ..
                        }) = &mut self.dialog
                        && contains(self.popup_rect, mouse.column, mouse.row)
                        && mouse.row >= self.popup_rect.y + 2
                    {
                        let index =
                            self.picker_start + (mouse.row - self.popup_rect.y - 2) as usize;
                        if index < choices.len() {
                            *selected = index;
                            return self
                                .dialog_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
                        }
                    };
                    return vec![];
                }
                match mouse.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        for (rect, index) in self.tabs.clone() {
                            if contains(rect, mouse.column, mouse.row) {
                                return self.switch_view(index);
                            }
                        }
                        if contains(self.scope_rect, mouse.column, mouse.row) {
                            self.scope_dialog();
                            return vec![];
                        }
                        if contains(self.search_rect, mouse.column, mouse.row) {
                            self.search_mode = SearchMode::Text;
                            self.search.clear();
                            self.local_search.clear();
                            return vec![];
                        }
                        if contains(self.assignee_rect, mouse.column, mouse.row) {
                            self.assignee_picker(true);
                            return vec![];
                        }
                        if contains(self.status_rect, mouse.column, mouse.row) {
                            self.status_picker(true);
                            return vec![];
                        }
                        for (rect, key) in self.footer_buttons.clone() {
                            if contains(rect, mouse.column, mouse.row) {
                                if self.search_mode == SearchMode::Text {
                                    self.remote_search = self.search.clone();
                                    self.search_due = None;
                                }
                                self.search_mode = SearchMode::None;
                                return self.event(Event::Key(KeyEvent::new(
                                    KeyCode::Char(key),
                                    KeyModifiers::NONE,
                                )));
                            }
                        }
                        if contains(self.tickets_rect, mouse.column, mouse.row)
                            && mouse.row > self.tickets_rect.y + 1
                        {
                            let n = self.table.offset()
                                + ((mouse.row - self.tickets_rect.y - 2) / self.row_height.max(1))
                                    as usize;
                            if n < self.visible.len() {
                                self.selected = n;
                                self.table.select(Some(n));
                                self.focus = Focus::Tickets;
                                self.search_mode = SearchMode::None;
                                self.search_due = None;
                                self.lookup_generation += 1;
                                self.select_current()
                            }
                        };
                        if contains(self.ticket_rect, mouse.column, mouse.row)
                            && mouse.column > self.ticket_rect.x
                            && mouse.column < self.ticket_rect.right() - 1
                            && mouse.row > self.ticket_rect.y
                            && mouse.row < self.ticket_rect.bottom() - 1
                        {
                            self.focus = Focus::Ticket;
                            let n = self.scroll + (mouse.row - self.ticket_rect.y - 1) as usize;
                            if let Some(target) = self
                                .document
                                .get(n)
                                .and_then(|r| r.target_at(mouse.column - self.ticket_rect.x - 1))
                            {
                                return self.open_target(target);
                            }
                        };
                    }
                    MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
                        let down = mouse.kind == MouseEventKind::ScrollDown;
                        if contains(self.ticket_rect, mouse.column, mouse.row) {
                            self.scroll = if down {
                                (self.scroll + 3).min(self.document.len().saturating_sub(1))
                            } else {
                                self.scroll.saturating_sub(3)
                            }
                        } else if contains(self.tickets_rect, mouse.column, mouse.row) {
                            self.selected = if down {
                                (self.selected + 3).min(self.visible.len().saturating_sub(1))
                            } else {
                                self.selected.saturating_sub(3)
                            };
                            self.table.select(Some(self.selected));
                            self.select_current()
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        vec![]
    }
    fn search_key(&mut self, key: KeyEvent) -> Vec<Job> {
        match key.code {
            KeyCode::Esc => {
                self.search_mode = SearchMode::None;
                self.lookup_active = false;
                self.search.clear();
                self.remote_search.clear();
                self.local_search.clear();
                self.search_due = None;
                self.lookup_generation += 1;
                return self.reload();
            }
            KeyCode::Enter => {
                if self.search_mode == SearchMode::Text {
                    self.remote_search = self.search.clone();
                    self.search_mode = SearchMode::None;
                    self.search_due = None;
                    return self.reload();
                };
                self.search_due = Some(Instant::now());
                let jobs = self.advance();
                self.search_mode = SearchMode::None;
                self.focus = Focus::Ticket;
                return jobs;
            }
            KeyCode::Backspace => {
                self.search.pop();
                self.search_changed()
            }
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                if self.search_mode == SearchMode::Number && !c.is_ascii_digit() {
                    let expected = data::number_key(self.settings.prefix(), &self.search);
                    if c != 'q'
                        && (self.search_due.is_some()
                            || self.loading
                            || expected.as_deref() != Some(&self.detail_key))
                    {
                        self.notice("Wait for this ticket lookup to complete", false);
                        return vec![];
                    }
                    self.search_mode = SearchMode::None;
                    self.search_due = None;
                    return self.event(Event::Key(key));
                };
                self.search.push(c);
                self.search_changed()
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.search.clear();
                self.search_changed()
            }
            _ => {}
        }
        vec![]
    }
    fn cached_backlog(&self) -> Vec<Ticket> {
        self.settings
            .query(config::BACKLOG, "", "", "")
            .ok()
            .and_then(|q| {
                self.cache
                    .get::<Vec<Ticket>>(&format!("list:{}:{q}", self.limit))
            })
            .map(|(items, _)| items)
            .unwrap_or_default()
    }
    pub fn known_statuses(&self) -> Vec<String> {
        let backlog = self.cached_backlog();
        let mut a: Vec<_> = self
            .items
            .iter()
            .chain(backlog.iter())
            .map(Ticket::status)
            .filter(|s| !s.is_empty())
            .collect();
        a.sort();
        a.dedup();
        a
    }
    fn scope_dialog(&mut self) {
        self.dialog = Some(Dialog::Form {
            title: "Advanced · shared backlog scope (already configured)".into(),
            labels: vec![
                "Project".into(),
                "Ticket prefix (blank = project)".into(),
                "Backlog scope JQL / saved filter (optional)".into(),
            ],
            values: vec![
                self.settings.project.clone(),
                self.settings.prefix.clone(),
                self.settings.scope_jql.clone(),
            ],
            selected: 0,
            kind: "scope".into(),
            key: String::new(),
        })
    }
    pub fn assignee_filter_value(&self) -> &str {
        match self.view {
            config::MINE => "@me",
            config::AVAILABLE => "unassigned",
            _ => &self.assignee_filter,
        }
    }
    pub fn assignee_label(&self) -> &str {
        match self.assignee_filter_value() {
            "@me" => "Me",
            "unassigned" => "Unassigned",
            "" => "All",
            v => v,
        }
    }
    fn apply_assignee_filter(&mut self, value: String) {
        if self.search_mode == SearchMode::Text {
            self.remote_search = self.search.clone();
        }
        if self.lookup_active {
            self.search.clear();
            self.local_search.clear();
            self.lookup_active = false;
        }
        self.search_mode = SearchMode::None;
        self.search_due = None;
        self.lookup_generation += 1;
        self.view = match value.as_str() {
            "@me" => config::MINE,
            "unassigned" => config::AVAILABLE,
            _ => config::BACKLOG,
        };
        self.assignee_filter = if self.view == config::BACKLOG {
            value
        } else {
            String::new()
        };
        self.settings.last_view = self.view;
        self.save_settings();
    }
    fn filters_dialog(&mut self) {
        self.dialog = Some(Dialog::Form {
            title: "Filters · blank clears · @me / unassigned / email or ID".into(),
            labels: vec!["Assignee".into(), "Status".into()],
            values: vec![
                self.assignee_filter_value().into(),
                self.status_filter.clone(),
            ],
            selected: 0,
            kind: "filters".into(),
            key: String::new(),
        })
    }
    fn assignee_picker(&mut self, filter: bool) {
        let mut choices = vec![
            Choice {
                label: "Me".into(),
                value: "@me".into(),
                target: None,
            },
            Choice {
                label: "Unassigned".into(),
                value: "unassigned".into(),
                target: None,
            },
        ];
        if filter {
            choices.insert(
                0,
                Choice {
                    label: "All assignees".into(),
                    value: String::new(),
                    target: None,
                },
            )
        };
        let mut seen = HashSet::new();
        let backlog = self.cached_backlog();
        for t in self.items.iter().chain(backlog.iter()) {
            let u = t.user("assignee");
            let id = u.identity(self.backend.kind == Kind::Jira);
            if !id.is_empty() && seen.insert(id.clone()) {
                choices.push(Choice {
                    label: format!(
                        "{} · {}",
                        u.label(),
                        if u.email.is_empty() { &id } else { &u.email }
                    ),
                    value: id,
                    target: None,
                })
            }
        }
        choices.push(Choice {
            label: "Other email / account ID…".into(),
            value: "__other".into(),
            target: None,
        });
        let selected = if filter {
            choices
                .iter()
                .position(|c| c.value == self.assignee_filter_value())
                .unwrap_or(0)
        } else {
            0
        };
        let key = self
            .active_ticket()
            .map(|t| t.key.clone())
            .unwrap_or_default();
        self.dialog = Some(Dialog::Picker {
            title: if filter {
                "Filter assignee"
            } else {
                "Assign · one ticket · confirmation required"
            }
            .into(),
            choices,
            selected,
            purpose: if filter {
                Purpose::AssigneeFilter
            } else {
                Purpose::Assign
            },
            key,
        })
    }
    fn status_picker(&mut self, filter: bool) {
        let mut statuses = if !filter {
            self.active_ticket()
                .map(|t| {
                    t.transitions
                        .iter()
                        .map(|v| data::text(&v["to"]["name"]))
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        } else {
            vec![]
        };
        let suggestions = statuses.is_empty();
        if suggestions {
            statuses = self.known_statuses()
        };
        statuses.sort();
        statuses.dedup();
        let mut choices = Vec::new();
        if filter {
            choices.push(Choice {
                label: "Any status".into(),
                value: String::new(),
                target: None,
            })
        }
        choices.extend(statuses.into_iter().map(|s| Choice {
            label: s.clone(),
            value: s,
            target: None,
        }));
        choices.push(Choice {
            label: "Other status…".into(),
            value: "__other".into(),
            target: None,
        });
        let key = self
            .active_ticket()
            .map(|t| t.key.clone())
            .unwrap_or_default();
        self.dialog = Some(Dialog::Picker {
            title: if filter {
                "Filter status"
            } else if suggestions {
                "Transition · suggestions; Jira validates workflow"
            } else {
                "Transition · Jira validates workflow"
            }
            .into(),
            choices,
            selected: 0,
            purpose: if filter {
                Purpose::StatusFilter
            } else {
                Purpose::Status
            },
            key,
        })
    }
    fn action_dialog(&mut self, c: char) {
        if self.read_only {
            self.notice("Read-only mode: changes are disabled", true);
            return;
        };
        if self.writing {
            self.notice("Another update is in flight", true);
            return;
        };
        if self.lookup_active
            && self.history.is_empty()
            && (self.search_due.is_some() || self.loading)
        {
            self.notice("Wait for the ticket lookup before editing", false);
            return;
        }
        let Some(ticket) = self.active_ticket().cloned() else {
            self.notice("Select a ticket first", true);
            return;
        };
        match c {
            'a' => self.assignee_picker(false),
            't' => self.status_picker(false),
            'e' => {
                self.dialog = Some(Dialog::Picker {
                    title: format!("Edit {}", ticket.key),
                    choices: [
                        ("Summary", "summary"),
                        ("Description (Markdown)", "description"),
                        ("Labels", "labels"),
                    ]
                    .into_iter()
                    .map(|(label, value)| Choice {
                        label: label.into(),
                        value: value.into(),
                        target: None,
                    })
                    .collect(),
                    selected: 0,
                    purpose: Purpose::Edit,
                    key: ticket.key,
                })
            }
            'c' => {
                self.dialog = Some(Dialog::Editor {
                    title: format!("Comment · {} · Markdown", ticket.key),
                    value: String::new(),
                    cursor: 0,
                    kind: "comment".into(),
                    key: ticket.key,
                })
            }
            'L' => {
                self.dialog = Some(Dialog::Form {
                    title: format!("Link {}", ticket.key),
                    labels: vec!["Target ticket key".into(), "Outward link type".into()],
                    values: vec![String::new(), "blocks".into()],
                    selected: 0,
                    kind: "link".into(),
                    key: ticket.key,
                })
            }
            _ => {}
        }
    }
    fn links(&mut self) {
        if let Some(t) = self.active_ticket() {
            let key = t.key.clone();
            let choices = t
                .targets()
                .into_iter()
                .map(|t| Choice {
                    label: t.label.clone(),
                    value: String::new(),
                    target: Some(t),
                })
                .collect();
            self.dialog = Some(Dialog::Picker {
                title: "Links & attachments · internal tickets / browser / local image".into(),
                choices,
                selected: 0,
                purpose: Purpose::Links,
                key,
            })
        }
    }
    fn confirm(&mut self, action: Action) {
        if data::require_key(&action.key).is_err()
            || (action.kind == "link" && data::require_key(&action.value).is_err())
        {
            self.notice("One valid ticket key is required", true);
            return;
        }
        if action.value.trim().is_empty() {
            self.notice("Empty update rejected", true);
            return;
        };
        self.dialog = Some(Dialog::Confirm(action))
    }
    fn open_target(&mut self, target: Target) -> Vec<Job> {
        if let Some(attachment) = &target.attachment {
            if attachment.mime.starts_with("image/")
                && let Some(path) = config::local_attachment(
                    &self.attachment_dir,
                    &self.detail_key,
                    &attachment.name,
                )
            {
                return self.preview_job(path);
            }
            self.notice(
                "Attachment opens in Jira; CLI has no authenticated download action",
                false,
            )
        }
        if let Some(key) = target.key {
            if !data::valid_key(&key) {
                self.notice("Invalid ticket link", true);
                return vec![];
            };
            if !self.detail_key.is_empty() && self.detail_key != key {
                self.history.push((self.detail_key.clone(), self.scroll))
            };
            self.detail_key = key.clone();
            self.scroll = 0;
            self.doc_dirty = true;
            self.focus = Focus::Ticket;
            return self.detail_job(key, None, false);
        }
        if let Some(url) = target.url
            && data::valid_url(&url)
        {
            return vec![Job::Browser(url)];
        }
        vec![]
    }
    fn preview_job(&mut self, path: PathBuf) -> Vec<Job> {
        self.image_generation += 1;
        self.awaiting_image = true;
        self.notice("Loading image…", false);
        vec![Job::Preview {
            generation: self.image_generation,
            path,
            size: (
                (self.tickets_rect.width + self.ticket_rect.width)
                    .saturating_sub(10)
                    .max(10),
                self.ticket_rect.height.saturating_sub(4).max(6),
            ),
        }]
    }
    fn save_settings(&mut self) {
        if self.backend.kind != Kind::Demo
            && let Err(e) = self.settings.save()
        {
            self.notice(format!("Cannot save settings: {e}"), true)
        }
    }
    fn dialog_key(&mut self, key: KeyEvent) -> Vec<Job> {
        let Some(mut dialog) = self.dialog.take() else {
            return vec![];
        };
        if key.code == KeyCode::Esc
            || (key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c'))
        {
            return vec![];
        }
        match &mut dialog {
            Dialog::Confirm(action) => {
                if key.code == KeyCode::Char('y') {
                    if self.read_only {
                        self.notice("Read-only update blocked", true);
                        return vec![];
                    };
                    self.writing = true;
                    self.notice(format!("Updating {}…", action.key), false);
                    return vec![Job::Write(action.clone())];
                }
                if key.code == KeyCode::Char('n') {
                    return vec![];
                }
            }
            Dialog::Picker {
                choices,
                selected,
                purpose,
                key: ticket_key,
                ..
            } => match key.code {
                KeyCode::Down | KeyCode::Char('j') => {
                    *selected = (*selected + 1).min(choices.len().saturating_sub(1))
                }
                KeyCode::Up | KeyCode::Char('k') => *selected = selected.saturating_sub(1),
                KeyCode::Enter => {
                    let Some(choice) = choices.get(*selected).cloned() else {
                        return vec![];
                    };
                    let purpose = *purpose;
                    let ticket_key = ticket_key.clone();
                    if purpose == Purpose::Links {
                        if let Some(t) = choice.target {
                            return self.open_target(t);
                        }
                        return vec![];
                    }
                    if choice.value == "__other" {
                        self.dialog = Some(Dialog::Form {
                            title: "Enter email/account ID or exact status".into(),
                            labels: vec!["Value".into()],
                            values: vec![String::new()],
                            selected: 0,
                            kind: match purpose {
                                Purpose::Assign => "assign",
                                Purpose::Status => "status",
                                Purpose::AssigneeFilter => "assignee_filter",
                                _ => "status_filter",
                            }
                            .into(),
                            key: ticket_key,
                        });
                        return vec![];
                    }
                    match purpose {
                        Purpose::Assign | Purpose::Status => {
                            self.confirm(Action::new(
                                if purpose == Purpose::Assign {
                                    "assign"
                                } else {
                                    "status"
                                },
                                &ticket_key,
                                &choice.value,
                            ));
                            return vec![];
                        }
                        Purpose::AssigneeFilter => {
                            self.apply_assignee_filter(choice.value);
                            return self.reload();
                        }
                        Purpose::StatusFilter => {
                            self.status_filter = choice.value;
                            return self.reload();
                        }
                        Purpose::Edit => {
                            let t = self
                                .details
                                .get(&ticket_key)
                                .map(|x| &x.0)
                                .or_else(|| self.selected_ticket());
                            let value = t
                                .map(|t| match choice.value.as_str() {
                                    "summary" => t.summary(),
                                    "description" => t.description(),
                                    "labels" => t.fields["labels"]
                                        .as_array()
                                        .map(|a| {
                                            a.iter().map(data::text).collect::<Vec<_>>().join(",")
                                        })
                                        .unwrap_or_default(),
                                    _ => String::new(),
                                })
                                .unwrap_or_default();
                            if choice.value == "description"
                                && !self.details.contains_key(&ticket_key)
                            {
                                self.notice(
                                    "Load full details before editing the description",
                                    true,
                                );
                                return self.detail_job(ticket_key, None, false);
                            }
                            if choice.value == "description" {
                                self.dialog=Some(Dialog::Editor{title:"Edit description · Markdown → ADF; unsupported media may change".into(),cursor:value.len(),value,kind:choice.value,key:ticket_key})
                            } else {
                                self.dialog = Some(Dialog::Form {
                                    title: format!("Edit {}", choice.value),
                                    labels: vec![choice.label],
                                    values: vec![value],
                                    selected: 0,
                                    kind: choice.value,
                                    key: ticket_key,
                                })
                            };
                            return vec![];
                        }
                        _ => {}
                    }
                }
                _ => {}
            },
            Dialog::Form {
                values,
                selected,
                kind,
                key: ticket_key,
                ..
            } => {
                let submit = (key.modifiers.contains(KeyModifiers::CONTROL)
                    && key.code == KeyCode::Char('s'))
                    || (key.code == KeyCode::Enter && *selected == values.len().saturating_sub(1));
                if submit {
                    let kind = kind.clone();
                    let values = values.clone();
                    let ticket_key = ticket_key.clone();
                    match kind.as_str() {
                        "scope" => {
                            self.settings.project = values[0].trim().to_uppercase();
                            self.settings.prefix = values[1].trim().to_uppercase();
                            self.settings.scope_jql = values[2].trim().into();
                            self.save_settings();
                            return self.reload();
                        }
                        "filters" => {
                            self.apply_assignee_filter(values[0].trim().into());
                            self.status_filter = values[1].trim().into();
                            return self.reload();
                        }
                        "assignee_filter" => {
                            self.apply_assignee_filter(values[0].trim().into());
                            return self.reload();
                        }
                        "status_filter" => {
                            self.status_filter = values[0].trim().into();
                            return self.reload();
                        }
                        "jql" => {
                            self.settings.scope_jql = values[0].trim().into();
                            self.save_settings();
                            return self.reload();
                        }
                        "image" => return self.preview_job(PathBuf::from(values[0].trim())),
                        _ => {
                            let mut action = Action::new(&kind, &ticket_key, values[0].trim());
                            if kind == "link" {
                                action.extra = values.get(1).cloned().unwrap_or_default()
                            };
                            self.confirm(action);
                            return vec![];
                        }
                    }
                }
                match key.code {
                    KeyCode::Tab | KeyCode::Enter => {
                        *selected = (*selected + 1) % values.len().max(1)
                    }
                    KeyCode::BackTab => {
                        *selected = (*selected + values.len() - 1) % values.len().max(1)
                    }
                    KeyCode::Backspace => {
                        values[*selected].pop();
                    }
                    KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        values[*selected].clear()
                    }
                    KeyCode::Char(c)
                        if !key
                            .modifiers
                            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                    {
                        values[*selected].push(c)
                    }
                    _ => {}
                }
            }
            Dialog::Editor {
                value,
                cursor,
                kind,
                key: ticket_key,
                ..
            } => {
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
                    self.confirm(Action::new(kind, ticket_key, value));
                    return vec![];
                };
                crate::input::key(value, cursor, key);
            }
            Dialog::Info { scroll, .. } => match key.code {
                KeyCode::Down | KeyCode::Char('j') => *scroll = scroll.saturating_add(1),
                KeyCode::Up | KeyCode::Char('k') => *scroll = scroll.saturating_sub(1),
                KeyCode::Enter | KeyCode::Char('q') => return vec![],
                _ => {}
            },
        }
        self.dialog = Some(dialog);
        vec![]
    }
}
fn contains(r: Rect, x: u16, y: u16) -> bool {
    x >= r.x && x < r.right() && y >= r.y && y < r.bottom()
}
pub const HELP: &str = "LAYOUT\nTickets | Ticket. Tab changes focus. Mouse selects rows and links.\n\nTHREE PREDEFINED VIEWS\nCtrl+1 Mine: assigned to you.\nCtrl+2 Available: unassigned.\nCtrl+3 Backlog: combined Board + Backlog ticket scope, all assignees.\nClick the tabs, or use Alt+1…3 / F1…F3 as terminal fallbacks.\nNo separate Board view, backlog-status mapping, or sprint settings.\n\nSEARCH\nType digits: project prefix inferred, lookup automatic across the scope.\n/ searches the current view on the server, not just loaded rows.\nEnter submits immediately; Esc clears search/restores the view.\n\nTICKETS\n↑↓ / j k navigate; Enter focuses details; Esc returns from linked ticket.\na assign · t status · e edit · c comment · L link tickets\nl links/attachments · O opens Jira · C loads all comments\n\nFILTERS\nf filters · A / T or top dropdowns select assignee/status.\nChanging assignee selects Mine, Available, or filtered Backlog.\ng / Ctrl+G opens optional advanced project/JQL settings.\n\nATTACHMENTS\nFiles open in Jira. Downloaded images preview locally; i selects a file.\nKitty graphics where supported, otherwise Unicode half-blocks.\n\nSAFETY\nUpdates target one ticket and need y confirmation. n/Esc cancels.\n--read-only blocks changes. ! shows the full CLI error.\nr refreshes · q quits (wait for writes to finish).";

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> App {
        let mut settings = Settings {
            project: "DEV".into(),
            last_view: 2,
            ..Default::default()
        };
        settings.prefix = "DEV".into();
        let b = Backend::new("demo", None).unwrap();
        let mut a = App::new(
            b,
            Cache { directory: None },
            settings,
            false,
            500,
            PathBuf::new(),
        );
        a.set_items(data::demo_tickets());
        for t in data::demo_tickets() {
            a.details.insert(t.key.clone(), (t, config::now()));
        }
        a
    }
    fn key(c: char, mods: KeyModifiers) -> Event {
        Event::Key(KeyEvent::new(KeyCode::Char(c), mods))
    }
    #[test]
    fn numbers_search_not_views() {
        let mut a = app();
        a.event(key('1', KeyModifiers::NONE));
        assert_eq!(a.search_mode as u8, SearchMode::Number as u8);
        assert_eq!(a.search, "1");
        assert_eq!(a.view, 2);
        a.event(key('0', KeyModifiers::NONE));
        a.event(key('1', KeyModifiers::NONE));
        assert_eq!(a.search, "101");
        a.search_due = Some(Instant::now());
        let jobs = a.advance();
        assert!(matches!(&jobs[0],Job::Detail{key,..} if key=="DEV-101"));
        a.event(key('2', KeyModifiers::CONTROL));
        assert_eq!(a.view, 1);
        assert_eq!(a.search_mode as u8, SearchMode::None as u8);
    }
    #[test]
    fn read_only_and_confirmation() {
        let mut a = app();
        a.read_only = true;
        a.action_dialog('a');
        assert!(a.dialog.is_none());
        a.confirm(Action::new("status", "DEV-101", "Done"));
        let jobs = a.event(key('y', KeyModifiers::NONE));
        assert!(jobs.is_empty());
        assert!(!a.writing);
        a.read_only = false;
        a.action_dialog('a');
        let jobs = a.event(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));
        assert!(jobs.is_empty());
        assert!(matches!(a.dialog, Some(Dialog::Confirm(_))));
        let jobs = a.event(key('y', KeyModifiers::NONE));
        assert!(
            matches!(&jobs[0],Job::Write(action) if action.kind=="assign"&&action.value=="@me")
        );
    }
    #[test]
    fn old_lookup_does_not_replace_new() {
        let mut a = app();
        a.lookup_generation = 7;
        a.message(Message::Detail {
            key: "DEV-101".into(),
            lookup: Some(6),
            result: Ok(data::demo_tickets().remove(0)),
        });
        assert_eq!(a.items.len(), 3);
    }
    #[test]
    fn search_scope_not_assigned_by_default() {
        let a = app();
        let q = a.query().unwrap();
        assert!(q.contains("project = \"DEV\""));
        assert!(!q.contains("currentUser"));
        assert!(!q.contains("sprint"));
    }
    #[test]
    fn number_enter_submits_and_assignment_needs_no_enter() {
        let mut a = app();
        for c in "101".chars() {
            a.event(key(c, KeyModifiers::NONE));
        }
        let jobs = a.event(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));
        assert!(matches!(&jobs[0],Job::Detail{key,..} if key=="DEV-101"));
        a.message(Message::Detail {
            key: "DEV-101".into(),
            lookup: Some(a.lookup_generation),
            result: Ok(data::demo_tickets().remove(0)),
        });
        assert!(a.focus == Focus::Ticket);
        let mut a = app();
        for c in "101".chars() {
            a.event(key(c, KeyModifiers::NONE));
        }
        a.search_due = None;
        a.message(Message::Detail {
            key: "DEV-101".into(),
            lookup: Some(a.lookup_generation),
            result: Ok(data::demo_tickets().remove(0)),
        });
        a.event(key('a', KeyModifiers::NONE));
        assert!(matches!(
            a.dialog,
            Some(Dialog::Picker {
                purpose: Purpose::Assign,
                ..
            })
        ));
    }
    #[test]
    fn predefined_views_need_no_status_mapping() {
        let mut a = app();
        assert_eq!(a.view_names(), vec!["Mine", "Available", "Backlog"]);
        for (keycode, view) in [
            ('1', config::MINE),
            ('2', config::AVAILABLE),
            ('3', config::BACKLOG),
        ] {
            let jobs = a.event(key(keycode, KeyModifiers::CONTROL));
            assert_eq!(a.view, view);
            assert!(a.dialog.is_none());
            assert!(matches!(jobs.first(), Some(Job::List { .. })));
        }
    }
    #[test]
    fn text_search_stays_in_its_assignment_view() {
        let mut a = app();
        a.view = config::AVAILABLE;
        a.event(key('/', KeyModifiers::NONE));
        for c in "title".chars() {
            a.event(key(c, KeyModifiers::NONE));
        }
        a.search_due = Some(Instant::now());
        let jobs = a.advance();
        assert_eq!(a.view, config::AVAILABLE);
        assert!(
            matches!(&jobs[0],Job::List{query,..} if query.contains("assignee IS EMPTY")&&query.contains("text ~ \"title\""))
        );
        a.apply_assignee_filter("@me".into());
        assert_eq!(a.view, config::MINE);
        a.apply_assignee_filter("sam".into());
        assert_eq!(a.view, config::BACKLOG);
        assert!(a.query().unwrap().contains("assignee = \"sam\""));
        a.switch_view(config::MINE);
        assert_eq!(a.remote_search, "title");
        assert!(a.assignee_filter.is_empty());
        assert!(a.query().unwrap().contains("currentUser()"));
    }
    #[test]
    fn paste_into_editor_is_not_a_command() {
        let mut a = app();
        a.action_dialog('c');
        a.event(Event::Paste("body\nyes; $(no-shell)".into()));
        let Some(Dialog::Editor { value, .. }) = a.dialog else {
            panic!()
        };
        assert_eq!(value, "body\nyes; $(no-shell)");
        assert!(!a.writing);
    }
    #[test]
    fn assignments_use_identity_not_display_name() {
        let mut a = app();
        a.assignee_picker(false);
        let Some(Dialog::Picker { choices, .. }) = a.dialog else {
            panic!()
        };
        assert!(choices.iter().any(|c| c.value == "alex"));
        assert!(!choices.iter().any(|c| c.value == "Alex"));
    }
}
