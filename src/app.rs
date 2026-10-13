use crate::{
    backend::{Action, Backend, Kind},
    config::{self, Cache, Settings},
    data::{self, Target, Ticket},
    filters::Selection,
    markdown::{self, DocLine},
    workspace::{Workspace, full_cache_key},
};
use anyhow::Result;
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use ratatui::{layout::Rect, widgets::TableState};
use ratatui_image::protocol::Protocol;
use std::{
    collections::{BTreeMap, HashSet, VecDeque},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

pub enum Job {
    Sync {
        generation: u64,
        base: String,
        mine: String,
        changed: Option<String>,
    },

    Detail {
        key: String,
        lookup: Option<u64>,
        revision: u64,
        background: bool,
    },
    Comments(String),
    Write(Action),
    Browser(String),
    CopyLink(crate::clipboard::Request),
    AttachmentImage {
        generation: u64,
        key: String,
        attachment: data::Attachment,
        cache: Option<PathBuf>,
        size: (u16, u16),
        inline: bool,
        cancel: tokio::sync::watch::Receiver<bool>,
    },
    Preview {
        generation: u64,
        path: PathBuf,
        size: (u16, u16),
    },
}
pub enum Message {
    Synced {
        generation: u64,
        result: Result<(Workspace, Vec<String>)>,
    },

    Detail {
        key: String,
        lookup: Option<u64>,
        revision: u64,
        background: bool,
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
    Copied(crate::clipboard::Completion),
    InlineImage {
        generation: u64,
        key: String,
        id: String,
        result: Result<crate::images::InlineImage>,
    },
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
#[derive(Clone)]
pub struct FilterInput {
    pub column: usize,
    pub value: String,
    pub cursor: usize,
}
pub enum Dialog {
    Filters {
        choices: [Vec<Choice>; 2],
        selected: [usize; 2],
        checked: [Selection; 2],
        active: usize,
        custom: Option<FilterInput>,
    },
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
pub enum InlineImageState {
    Pending,
    Ready(crate::images::InlineImage),
    Failed(String),
}
#[derive(Clone, Default)]
pub struct ViewFilters {
    assignee: Selection,
    status: Selection,
    text: String,
}
pub struct App {
    pub status_colors: crate::appearance::StatusColors,
    view_filters: [ViewFilters; 4],
    pub backend: Arc<Backend>,
    pub cache: Cache,
    pub settings: Settings,
    pub read_only: bool,
    pub limit: usize,
    pub view: usize,
    pub items: Vec<Ticket>,
    pub workspace: Option<Workspace>,
    pub items_are_workspace: bool,
    pub syncing: bool,
    pub full_sync: bool,
    pub refresh_pending: bool,
    pub next_sync: Option<Instant>,
    pub dirty: bool,
    pub prefetch: VecDeque<String>,
    pub background: HashSet<(String, u64)>,
    pub dirty_details: HashSet<String>,
    pub full_ready: HashSet<String>,
    pub detail_revisions: BTreeMap<String, u64>,
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
    pub assignee_filter: Selection,
    pub status_filter: Selection,
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
    pub automatic_images: bool,
    pub inline_key: String,
    pub inline_size: (u16, u16),
    pub inline_generation: u64,
    pub inline_slots: Vec<crate::images::ImageSlot>,
    pub inline_heights: BTreeMap<String, u16>,
    pub inline_images: BTreeMap<String, InlineImageState>,
    inline_cancel: tokio::sync::watch::Sender<bool>,
    preview_cancel: tokio::sync::watch::Sender<bool>,
    pub image_generation: u64,
    pub copy_generation: u64,
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
    pub filter_list_rects: [Rect; 2],
    pub filter_list_starts: [usize; 2],
    pub filter_apply_rect: Rect,
    pub filter_reset_rect: Rect,
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
        let status_colors = cache
            .get::<crate::appearance::StatusColors>(crate::appearance::STATUS_COLORS_CACHE)
            .map(|(c, _)| c)
            .filter(crate::appearance::StatusColors::valid)
            .unwrap_or_default();
        Self {
            status_colors,
            view_filters: std::array::from_fn(|_| ViewFilters::default()),
            backend,
            cache,
            settings,
            read_only,
            limit,
            view,
            items: vec![],
            workspace: None,
            items_are_workspace: false,
            syncing: false,
            full_sync: false,
            refresh_pending: false,
            next_sync: None,
            dirty: false,
            prefetch: VecDeque::new(),
            background: HashSet::new(),
            dirty_details: HashSet::new(),
            full_ready: HashSet::new(),
            detail_revisions: BTreeMap::new(),
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
            assignee_filter: Selection::default(),
            status_filter: Selection::default(),
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
            automatic_images: true,
            inline_key: String::new(),
            inline_size: (0, 0),
            inline_generation: 0,
            inline_slots: vec![],
            inline_heights: BTreeMap::new(),
            inline_images: BTreeMap::new(),
            inline_cancel: tokio::sync::watch::channel(false).0,
            preview_cancel: tokio::sync::watch::channel(false).0,
            image_generation: 0,
            copy_generation: 0,
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
            filter_list_rects: [Rect::default(); 2],
            filter_list_starts: [0; 2],
            filter_apply_rect: Rect::default(),
            filter_reset_rect: Rect::default(),
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
        self.settings.query_selected(
            self.view,
            &self.remote_search,
            &self.assignee_filter,
            &self.status_filter,
        )
    }
    pub fn reload(&mut self) -> Vec<Job> {
        match self.settings.query(config::BACKLOG, "", "", "") {
            Ok(q) => {
                if self.last_query == q && self.workspace.is_some() {
                    self.apply_local();
                    return vec![];
                }
                self.load(q)
            }
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
    fn workspace_key(&self) -> String {
        match self.settings.board_id {
            Some(id) => format!(
                "workspace-board-v1:{id}:{}:{}",
                self.backend.board_identity().unwrap_or_default(),
                self.last_query
            ),
            None => format!("workspace-v1:{}", self.last_query),
        }
    }
    fn dirty_key(&self) -> String {
        if self.settings.board_id.is_some() {
            format!("workspace-dirty:{}", self.workspace_key())
        } else {
            format!("workspace-dirty:{}", self.last_query)
        }
    }
    fn save_dirty(&self) {
        let _ = self.cache.put(&self.dirty_key(), &self.dirty_details);
    }
    fn load(&mut self, query: String) -> Vec<Job> {
        if !self.last_query.is_empty() && self.last_query != query {
            self.view_filters = std::array::from_fn(|_| ViewFilters::default());
            self.assignee_filter.clear();
            self.status_filter.clear();
            self.search.clear();
            self.remote_search.clear();
            self.local_search.clear();
            self.search_mode = SearchMode::None;
            self.lookup_active = false;
            self.lookup_generation += 1;
            self.search_due = None;
        }
        self.generation += 1;
        self.last_query = query.clone();
        self.prefetch.clear();
        self.syncing = false;
        self.full_sync = false;
        self.refresh_pending = false;
        self.workspace = None;
        self.items_are_workspace = false;
        self.dirty_details = self
            .cache
            .get::<HashSet<String>>(&self.dirty_key())
            .map(|(v, _)| v)
            .unwrap_or_default();
        if let Some((workspace, _)) = self.cache.get::<Workspace>(&self.workspace_key()) {
            self.workspace = Some(workspace)
        } else if self.settings.board_id.is_none()
            && let Some((items, time)) = self
                .cache
                .get::<Vec<Ticket>>(&format!("list:{}:{query}", self.limit))
        {
            let mine = self
                .settings
                .query(config::MINE, "", "", "")
                .ok()
                .and_then(|q| {
                    self.cache
                        .get::<Vec<Ticket>>(&format!("list:{}:{q}", self.limit))
                })
                .map(|(items, _)| items.into_iter().map(|t| t.key).collect())
                .unwrap_or_default();
            self.workspace = Some(Workspace {
                items,
                mine,
                complete: false,
                synced_at: time,
                ..Default::default()
            });
        }
        if let Some(workspace) = &mut self.workspace {
            // Apply the same inventory to older cached snapshots immediately.
            workspace.items.retain(Ticket::backlog_ticket);
            workspace.reindex();
            if self.status_colors.register(&workspace.items) {
                let _ = self
                    .cache
                    .put(crate::appearance::STATUS_COLORS_CACHE, &self.status_colors);
            }
        }
        self.apply_local();
        self.sync_workspace()
    }
    fn apply_local(&mut self) {
        if !self.lookup_active {
            if !self.items_are_workspace {
                let items = self
                    .workspace
                    .as_ref()
                    .map(|w| w.items.clone())
                    .unwrap_or_default();
                self.set_items(items);
                self.items_are_workspace = true;
            } else {
                self.rebuild_visible();
                self.select_current();
            }
        }
        self.local_notice();
    }
    fn local_notice(&mut self) {
        self.notice(
            format!(
                "{} shown · {} in {} · {} locally ready{}{}{}",
                self.visible.len(),
                self.workspace
                    .as_ref()
                    .map(|w| w.view_count(self.view))
                    .unwrap_or(0),
                config::VIEW_NAMES[self.view],
                self.workspace.as_ref().map(|w| w.items.len()).unwrap_or(0),
                if self.view == config::BOARD
                    && self.workspace.as_ref().is_none_or(|w| w.board.is_none())
                {
                    " · Board membership unavailable"
                } else {
                    ""
                },
                if self.workspace.as_ref().is_some_and(|w| !w.complete) {
                    " · partial scope"
                } else {
                    ""
                },
                if !self.assignee_filter.is_empty()
                    || !self.status_filter.is_empty()
                    || !self.local_search.is_empty()
                    || !self.remote_search.is_empty()
                {
                    " · filters active (F6 clears)"
                } else {
                    ""
                }
            ),
            false,
        );
    }
    pub fn clear_filters(&mut self) -> Vec<Job> {
        self.assignee_filter.clear();
        self.status_filter.clear();
        self.remote_search.clear();
        self.local_search.clear();
        self.search.clear();
        self.search_mode = SearchMode::None;
        self.lookup_active = false;
        self.lookup_generation += 1;
        self.search_due = None;
        self.selected = 0;
        *self.table.offset_mut() = 0;
        self.reload()
    }
    /// Pull the complete scope again and bypass every scoped detail cache.
    /// Keep the old snapshot visible; never discard useful data on failure.
    pub fn refresh_all(&mut self) -> Vec<Job> {
        if self.writing {
            self.notice("Wait for the in-flight update before refreshing", true);
            return vec![];
        }
        self.inline_cancel.send_replace(true);
        self.inline_cancel = tokio::sync::watch::channel(false).0;
        self.inline_generation += 1;
        self.inline_images
            .retain(|_, state| matches!(state, InlineImageState::Ready(_)));
        if self.last_query.is_empty() || self.settings.scope().is_err() {
            return self.reload();
        }
        if self.syncing {
            if !self.full_sync {
                self.refresh_pending = true;
            }
            self.notice(
                if self.full_sync {
                    "Full refresh already running"
                } else {
                    "Full refresh queued after current sync"
                },
                false,
            );
            return vec![];
        }
        self.refresh_pending = false;
        self.generation += 1;
        self.prefetch.clear();
        let mut keys = self
            .workspace
            .as_ref()
            .map(Workspace::keys)
            .unwrap_or_default();
        if !self.detail_key.is_empty() {
            keys.insert(self.detail_key.clone());
        }
        for key in keys {
            *self.detail_revisions.entry(key.clone()).or_default() += 1;
            self.pending.remove(&key);
            self.dirty_details.insert(key);
        }
        self.save_dirty();
        let jobs = self.sync_workspace_mode(true);
        self.notice(
            "Refreshing entire workspace; cached tickets remain visible",
            false,
        );
        jobs
    }
    pub fn sync_workspace(&mut self) -> Vec<Job> {
        self.sync_workspace_mode(false)
    }
    fn sync_workspace_mode(&mut self, full: bool) -> Vec<Job> {
        if self.syncing {
            return vec![];
        }
        let Ok(mine) = self.settings.query(config::MINE, "", "", "") else {
            return vec![];
        };
        let changed = if full {
            None
        } else {
            self.workspace.as_ref().map(|w| {
                Workspace::recent_query(
                    &self.last_query,
                    config::now().saturating_sub(w.synced_at) / 60 + 2,
                )
            })
        };
        self.full_sync = full;
        self.syncing = true;
        self.loading = self.workspace.is_none();
        self.next_sync = Some(Instant::now() + Duration::from_secs(60));
        vec![Job::Sync {
            generation: self.generation,
            base: self.last_query.clone(),
            mine,
            changed,
        }]
    }
    fn set_items(&mut self, items: Vec<Ticket>) {
        let key = self.selected_ticket().map(|t| t.key.clone());
        self.items = items;
        self.rebuild_visible();
        if let Some(key) = key
            && let Some(n) = self.visible.iter().position(|n| self.items[*n].key == key)
        {
            self.selected = n
        }
        self.table
            .select((!self.visible.is_empty()).then_some(self.selected));
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
        let q = if self.search_mode == SearchMode::Text || self.search_mode == SearchMode::Number {
            self.local_search.as_str()
        } else {
            self.remote_search.as_str()
        }
        .to_lowercase();
        let words: Vec<_> = q.split_whitespace().collect();
        let assignee = self.assignee_filter.normalized();
        let status = self.status_filter.normalized();
        let prefix = format!("{}-{q}", self.settings.prefix()).to_lowercase();
        self.visible = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(n, t)| {
                let matches = if self.search_mode == SearchMode::Number {
                    t.key.to_lowercase().starts_with(&prefix)
                } else {
                    self.lookup_active
                        || self
                            .workspace
                            .as_ref()
                            .map(|w| w.matches_words(t, self.view, &assignee, &status, &words))
                            .unwrap_or_else(|| {
                                q.is_empty() || t.summary().to_lowercase().contains(&q)
                            })
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
            if self.detail_key == key {
                return;
            }
            self.load_cached_detail(&key);
            self.detail_key = key;
            self.history.clear();
            self.scroll = 0;
            self.doc_dirty = true;
            self.selection_due = Some(Instant::now() + Duration::from_millis(80));
            for index in self.visible.iter().skip(self.selected + 1).take(8).rev() {
                let key = self.items[*index].key.clone();
                self.prefetch.push_front(key);
            }
        } else {
            self.detail_key.clear();
            self.doc_dirty = true
        }
    }
    fn load_cached_detail(&mut self, key: &str) {
        if !self.details.contains_key(key)
            && let Some((mut t, time)) = self
                .cache
                .get::<Ticket>(&full_cache_key(key))
                .or_else(|| self.cache.get(&format!("detail:{key}")))
        {
            if let Some(row) = self.workspace.as_ref().and_then(|w| w.ticket(key))
                && !row.estimate_fields.is_empty()
            {
                t.update_estimates(row);
            }
            if self.status_colors.register([&t]) {
                let _ = self
                    .cache
                    .put(crate::appearance::STATUS_COLORS_CACHE, &self.status_colors);
            }
            self.details.insert(key.into(), (t, time));
            self.trim_details(key);
            self.doc_dirty = true;
        }
    }
    fn trim_details(&mut self, keep: &str) {
        while self.details.len() > 64 {
            let old = self
                .details
                .iter()
                .filter(|(k, _)| k.as_str() != keep && *k != &self.detail_key)
                .min_by_key(|(_, (_, time))| *time)
                .map(|(k, _)| k.clone());
            if let Some(old) = old {
                self.details.remove(&old);
            } else {
                break;
            }
        }
    }
    fn detail_job(&mut self, key: String, lookup: Option<u64>, force: bool) -> Vec<Job> {
        let force = force
            || self
                .workspace
                .as_ref()
                .is_some_and(|w| !w.items.iter().any(|t| t.key == key));
        if self.pending.contains(&key) {
            return vec![];
        };
        self.load_cached_detail(&key);
        if !force
            && !self.dirty_details.contains(&key)
            && (self.cache.exists(&full_cache_key(&key))
                || self.full_ready.contains(&key) && self.details.contains_key(&key))
        {
            return vec![];
        }
        if !force
            && !self.dirty_details.contains(&key)
            && self.backend.kind == Kind::Demo
            && self.details.contains_key(&key)
        {
            return vec![];
        }
        self.pending.insert(key.clone());
        let revision = *self.detail_revisions.get(&key).unwrap_or(&0);
        vec![Job::Detail {
            key,
            lookup,
            revision,
            background: false,
        }]
    }
    pub fn advance(&mut self) -> Vec<Job> {
        let now = Instant::now();
        let mut jobs = Vec::new();
        if self.search_due.is_some_and(|d| now >= d) {
            self.search_due = None;
            self.dirty = true;
            match self.search_mode {
                SearchMode::Number => {
                    if let Some(key) = data::number_key(self.settings.prefix(), &self.search) {
                        self.load_cached_detail(&key);
                        let cached = self.details.get(&key).map(|(t, _)| t.clone()).or_else(|| {
                            self.workspace
                                .as_ref()
                                .and_then(|w| w.items.iter().find(|t| t.key == key).cloned())
                        });
                        if let Some(ticket) = cached {
                            self.show_lookup(ticket);
                            self.loading = false;
                        } else {
                            self.loading = true;
                        }
                        let outside = self
                            .workspace
                            .as_ref()
                            .is_none_or(|w| !w.items.iter().any(|t| t.key == key));
                        jobs.extend(self.detail_job(key, Some(self.lookup_generation), outside));
                    }
                }
                SearchMode::Text => {} // local search never waits for a timer
                _ => {}
            }
        }
        if self.selection_due.is_some_and(|d| now >= d) {
            self.selection_due = None;
            if !self.detail_key.is_empty() {
                jobs.extend(self.detail_job(self.detail_key.clone(), None, false));
            }
        }
        if self.next_sync.is_some_and(|d| now >= d) && !self.syncing && !self.writing {
            jobs.extend(self.sync_workspace());
        }
        if !self.writing {
            jobs.extend(self.inline_jobs());
        }
        if !self.syncing && !self.writing {
            while self.background.len() < 2 {
                let Some(key) = self.prefetch.pop_front() else {
                    break;
                };
                if self.pending.contains(&key) {
                    continue;
                }
                if !self.dirty_details.contains(&key)
                    && (self.cache.exists(&full_cache_key(&key)) || self.full_ready.contains(&key))
                {
                    continue;
                }
                self.pending.insert(key.clone());
                self.background
                    .insert((key.clone(), *self.detail_revisions.get(&key).unwrap_or(&0)));
                jobs.push(Job::Detail {
                    revision: *self.detail_revisions.get(&key).unwrap_or(&0),
                    key,
                    lookup: None,
                    background: true,
                });
            }
        }
        jobs
    }
    fn inline_jobs(&mut self) -> Vec<Job> {
        if !self.automatic_images || self.inline_key != self.detail_key {
            return vec![];
        }
        let height = self.ticket_rect.height.saturating_sub(2) as usize;
        let visible = self
            .inline_slots
            .iter()
            .filter(|s| s.row < self.scroll + height && s.row + s.height as usize > self.scroll)
            .map(|s| s.attachment.id.clone())
            .collect::<HashSet<_>>();
        self.inline_images
            .retain(|id, state| visible.contains(id) || matches!(state, InlineImageState::Pending));
        let mut pending = self
            .inline_images
            .values()
            .filter(|s| matches!(s, InlineImageState::Pending))
            .count();
        let mut jobs = vec![];
        for slot in &self.inline_slots {
            if pending == 2 {
                break;
            }
            if !visible.contains(&slot.attachment.id)
                || self.inline_images.contains_key(&slot.attachment.id)
            {
                continue;
            }
            self.inline_images
                .insert(slot.attachment.id.clone(), InlineImageState::Pending);
            pending += 1;
            jobs.push(Job::AttachmentImage {
                generation: self.inline_generation,
                key: self.detail_key.clone(),
                attachment: slot.attachment.clone(),
                cache: crate::images::cache_path(
                    self.cache.directory.as_deref(),
                    &self.detail_key,
                    &slot.attachment.id,
                ),
                size: self.inline_size,
                inline: true,
                cancel: self.inline_cancel.subscribe(),
            });
        }
        jobs
    }
    fn show_lookup(&mut self, ticket: Ticket) {
        let key = ticket.key.clone();
        self.items = vec![ticket];
        self.items_are_workspace = false;
        self.local_search.clear();
        self.visible = vec![0];
        self.selected = 0;
        self.table.select(Some(0));
        self.detail_key = key.clone();
        self.scroll = 0;
        self.doc_dirty = true;
        self.notice(
            format!("Found {key} locally · a assigns · Esc restores list"),
            false,
        );
    }
    pub fn message(&mut self, message: Message) -> Vec<Job> {
        match message {
            Message::Synced { generation, result } => {
                if generation != self.generation {
                    return vec![];
                }
                self.syncing = false;
                let full = self.full_sync;
                self.full_sync = false;
                self.loading = false;
                match result {
                    Ok((mut workspace, changed)) => {
                        let count = workspace.items.len();
                        workspace.items.retain(Ticket::backlog_ticket);
                        if workspace.index.is_empty() || count != workspace.items.len() {
                            workspace.reindex();
                        }
                        if self.status_colors.register(&workspace.items) {
                            let _ = self
                                .cache
                                .put(crate::appearance::STATUS_COLORS_CACHE, &self.status_colors);
                        }
                        let keys = workspace.keys();
                        if full {
                            self.dirty_details.extend(keys.iter().cloned());
                        }
                        self.dirty_details.extend(changed);
                        if let Some(old) = &self.workspace {
                            for ticket in &workspace.items {
                                if old
                                    .items
                                    .iter()
                                    .find(|t| t.key == ticket.key)
                                    .is_none_or(|t| t.fields != ticket.fields)
                                {
                                    self.dirty_details.insert(ticket.key.clone());
                                }
                            }
                        }
                        self.dirty_details.retain(|k| keys.contains(k));
                        for key in &self.dirty_details {
                            if self.pending.contains(key) {
                                *self.detail_revisions.entry(key.clone()).or_default() += 1;
                                self.pending.remove(key);
                            }
                        }
                        if self.dirty_details.contains(&self.detail_key)
                            || full && !self.detail_key.is_empty()
                        {
                            self.selection_due = Some(Instant::now());
                        }
                        self.save_dirty();
                        let _ = self.cache.put(&self.workspace_key(), &workspace);
                        self.prefetch = workspace.items.iter().map(|t| t.key.clone()).collect();
                        for ticket in &workspace.items {
                            if let Some((cached, _)) = self.details.get_mut(&ticket.key) {
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
                                    if let Some(value) = ticket.fields.get(field) {
                                        cached.fields[field] = value.clone();
                                    }
                                }
                                if !ticket.estimate_fields.is_empty() {
                                    cached.update_estimates(ticket);
                                }
                            }
                        }
                        self.workspace = Some(workspace);
                        self.items_are_workspace = false;
                        self.apply_local();
                        self.refresh_filter_lists();
                        self.doc_dirty = true;
                    }
                    Err(e) => {
                        self.prefetch.clear();
                        self.notice(
                            format!("Background sync failed; cached tickets retained: {e}"),
                            true,
                        );
                        self.next_sync = Some(Instant::now() + Duration::from_secs(120));
                    }
                }
                if self.refresh_pending {
                    return self.refresh_all();
                }
            }
            Message::Detail {
                key,
                lookup,
                revision,
                background,
                result,
            } => {
                self.background.remove(&(key.clone(), revision));
                if revision != *self.detail_revisions.get(&key).unwrap_or(&0) {
                    return vec![];
                }
                self.pending.remove(&key);
                if lookup.is_some() && lookup != Some(self.lookup_generation) {
                    return vec![];
                };
                if lookup.is_some() {
                    self.loading = false
                };
                match result {
                    Ok(ticket) => {
                        if self.status_colors.register([&ticket]) {
                            let _ = self
                                .cache
                                .put(crate::appearance::STATUS_COLORS_CACHE, &self.status_colors);
                        }
                        let _ = self.cache.put(&full_cache_key(&key), &ticket);
                        self.dirty_details.remove(&key);
                        self.full_ready.insert(key.clone());
                        self.save_dirty();
                        if let Some(workspace) = &mut self.workspace {
                            workspace.update(&ticket)
                        }
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
                                    "description",
                                ] {
                                    item.fields[field] = ticket.fields[field].clone();
                                }
                                item.update_estimates(&ticket);
                            }
                        }
                        self.trim_details(&key);
                        if lookup.is_some() {
                            self.show_lookup(ticket);
                        }
                        if !self.lookup_active {
                            self.rebuild_visible();
                        }
                        if self.detail_key == key {
                            self.doc_dirty = true
                        }
                    }
                    Err(e) => {
                        if background {
                            self.prefetch.clear();
                        }
                        if !background && (lookup.is_some() || self.detail_key == key) {
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
                            let _ = self.cache.put(&full_cache_key(&key), ticket);
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
                self.cache.remove(&full_cache_key(&action.key));
                *self.detail_revisions.entry(action.key.clone()).or_default() += 1;
                self.pending.remove(&action.key);
                self.full_ready.remove(&action.key);
                self.dirty_details.insert(action.key.clone());
                self.save_dirty();
                self.generation += 1;
                self.syncing = false;
                self.cache
                    .remove(&format!("list:{}:{}", self.limit, self.last_query));
                match result {
                    Ok(()) => {
                        self.notice(format!("Updated {}", action.key), false);
                        let mut jobs = self.sync_workspace();
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
            Message::Copied(completion) => {
                if completion.request.generation != self.copy_generation {
                    return vec![];
                }
                match completion.result {
                    Ok(crate::clipboard::Method::Native) => {
                        self.notice("Ticket link copied", false)
                    }
                    Ok(crate::clipboard::Method::Terminal) => {
                        self.notice("Ticket link sent to terminal clipboard (OSC52)", false)
                    }
                    Err(_) => {
                        self.notice("Could not copy ticket link; check clipboard support", true)
                    }
                }
            }
            Message::Notice(result) => match result {
                Ok(text) => self.notice(text, false),
                Err(e) => self.notice(e.to_string(), true),
            },
            Message::InlineImage {
                generation,
                key,
                id,
                result,
            } => {
                if generation != self.inline_generation
                    || key != self.detail_key
                    || key != self.inline_key
                    || !self.inline_images.contains_key(&id)
                {
                    return vec![];
                }
                self.inline_images.insert(
                    id.clone(),
                    match result {
                        Ok(p) => {
                            let height = p.protocol.size().height;
                            if let Some(slot) =
                                self.inline_slots.iter().find(|s| s.attachment.id == id)
                                && slot.row + slot.height as usize <= self.scroll
                            {
                                self.scroll = self
                                    .scroll
                                    .saturating_add(height as usize)
                                    .saturating_sub(slot.height as usize);
                            }
                            self.inline_heights.insert(id.clone(), height);
                            self.doc_dirty = true;
                            InlineImageState::Ready(p)
                        }
                        Err(e) => InlineImageState::Failed(data::line(&e.to_string())),
                    },
                );
            }
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
        let height = self.ticket_rect.height.saturating_sub(3).clamp(3, 10);
        let mut anchor = None;
        if self.inline_key != self.detail_key || self.inline_size != (width, height) {
            self.inline_cancel.send_replace(true);
            self.inline_cancel = tokio::sync::watch::channel(false).0;
            self.inline_generation += 1;
            if self.inline_key == self.detail_key && self.inline_size.0 > 0 {
                let old_width = usize::from(self.inline_size.0);
                anchor = self
                    .inline_slots
                    .iter()
                    .find(|s| s.row <= self.scroll && self.scroll < s.row + s.height as usize)
                    .map(|s| {
                        (
                            s.attachment.id.clone(),
                            (self.scroll - s.row) * usize::from(width) / old_width,
                        )
                    });
                for h in self.inline_heights.values_mut() {
                    *h = (usize::from(*h) * usize::from(width))
                        .div_ceil(old_width)
                        .clamp(1, 4096) as u16;
                }
            } else {
                self.inline_heights.clear();
            }
            self.inline_key = self.detail_key.clone();
            self.inline_size = (width, height);
            self.inline_images.clear();
            self.inline_slots.clear();
            self.doc_dirty = true;
        }
        if self.doc_dirty || self.doc_width != width {
            self.document = if let Some((ticket, _)) = self.details.get(&self.detail_key) {
                markdown::ticket_document_with_status(
                    ticket,
                    width,
                    self.status_colors.style(ticket),
                )
            } else if let Some(ticket) = self.items.iter().find(|t| t.key == self.detail_key) {
                let mut lines = markdown::render(
                    "*Description ready locally · full details syncing in background*",
                    width,
                    ticket,
                );
                lines.extend(markdown::ticket_document_with_status(
                    ticket,
                    width,
                    self.status_colors.style(ticket),
                ));
                lines
            } else {
                vec![]
            };
            let previous_ids = self
                .inline_slots
                .iter()
                .map(|s| s.attachment.id.clone())
                .collect::<Vec<_>>();
            self.inline_slots = if self.automatic_images {
                self.details
                    .get(&self.detail_key)
                    .map(|(t, _)| t)
                    .or_else(|| self.items.iter().find(|t| t.key == self.detail_key))
                    .map(|t| {
                        crate::images::insert_rows(
                            &mut self.document,
                            t,
                            height,
                            &self.inline_heights,
                        )
                    })
                    .unwrap_or_default()
            } else {
                vec![]
            };
            if !self.inline_images.is_empty()
                && previous_ids
                    != self
                        .inline_slots
                        .iter()
                        .map(|s| s.attachment.id.clone())
                        .collect::<Vec<_>>()
            {
                self.inline_cancel.send_replace(true);
                self.inline_cancel = tokio::sync::watch::channel(false).0;
                self.inline_generation += 1;
                self.inline_images.clear();
            }
            self.inline_heights
                .retain(|id, _| self.inline_slots.iter().any(|s| s.attachment.id == *id));
            if let Some((id, offset)) = anchor
                && let Some(slot) = self.inline_slots.iter().find(|s| s.attachment.id == id)
            {
                self.scroll = slot.row + offset.min(slot.height.saturating_sub(1) as usize);
            }
            self.doc_width = width;
            self.doc_dirty = false;
            self.scroll = self.scroll.min(self.document.len().saturating_sub(1));
        }
    }
    fn switch_view(&mut self, index: usize) -> Vec<Job> {
        if index >= self.view_names().len() {
            return vec![];
        };
        let text = if self.search_mode == SearchMode::Text {
            self.search.clone()
        } else {
            self.remote_search.clone()
        };
        self.view_filters[self.view] = ViewFilters {
            assignee: self.assignee_filter.clone(),
            status: self.status_filter.clone(),
            text,
        };
        let filters = self.view_filters[index].clone();
        self.view = index;
        self.assignee_filter = filters.assignee;
        self.status_filter = filters.status;
        self.remote_search = filters.text;
        self.selected = 0;
        self.visible.clear();
        self.table.select(None);
        *self.table.offset_mut() = 0;
        self.settings.last_view = index;
        if self.backend.kind != Kind::Demo {
            let _ = self.settings.save();
        };
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
        self.lookup_generation += 1;
        self.local_search = self.search.clone();
        self.selected = 0;
        self.loading = false;
        self.dirty = true;
        self.search_due = None;
        if self.search_mode == SearchMode::Text {
            self.lookup_active = false;
            self.apply_local();
            return;
        }
        if self.search_mode == SearchMode::Number && self.search.is_empty() {
            self.lookup_active = false;
            self.search_mode = SearchMode::None;
            self.remote_search.clear();
            self.apply_local();
            return;
        }
        if self.search_mode == SearchMode::Number
            && let Some(key) = data::number_key(self.settings.prefix(), &self.search)
        {
            let cached = self
                .workspace
                .as_ref()
                .and_then(|w| w.ticket(&key))
                .cloned()
                .or_else(|| self.details.get(&key).map(|(t, _)| t.clone()));
            let cached = if cached.is_none()
                && (self.cache.exists(&full_cache_key(&key))
                    || self.cache.exists(&format!("detail:{key}")))
            {
                self.load_cached_detail(&key);
                self.details.get(&key).map(|(t, _)| t.clone())
            } else {
                cached
            };
            if let Some(ticket) = cached {
                self.show_lookup(ticket);
                self.selection_due = Some(Instant::now() + Duration::from_millis(80));
                return;
            }
            if !self.items_are_workspace {
                self.items = self
                    .workspace
                    .as_ref()
                    .map(|w| w.items.clone())
                    .unwrap_or_default();
                self.items_are_workspace = true;
            }
            self.rebuild_visible();
            self.select_current();
            // Only an unknown/out-of-scope number needs a debounced CLI lookup.
            self.search_due = Some(Instant::now() + Duration::from_millis(300));
        }
    }
    pub fn event(&mut self, event: Event) -> Vec<Job> {
        match event {
            Event::Paste(text) => {
                let text = data::safe(&text);
                match &mut self.dialog {
                    Some(Dialog::Filters {
                        custom: Some(input),
                        ..
                    }) => crate::input::insert(&mut input.value, &mut input.cursor, &text),
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
                self.awaiting_image = false;
                self.preview_cancel.send_replace(true);
                self.preview_cancel = tokio::sync::watch::channel(false).0;
            }
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                if self.preview.is_some() || self.awaiting_image {
                    if matches!(key.code, KeyCode::Esc | KeyCode::Char('q')) {
                        self.preview = None;
                        self.awaiting_image = false;
                        self.image_generation += 1;
                        self.preview_cancel.send_replace(true);
                        self.preview_cancel = tokio::sync::watch::channel(false).0;
                    };
                    return vec![];
                }
                if self.dialog.is_some() {
                    return self.dialog_key(key);
                }
                if key.code == KeyCode::F(5)
                    || key.modifiers.contains(KeyModifiers::CONTROL)
                        && matches!(key.code, KeyCode::Char('r' | 'R'))
                {
                    return self.refresh_all();
                }
                if key.code == KeyCode::F(6) {
                    return self.clear_filters();
                }
                if key.modifiers.is_empty() && self.search_mode != SearchMode::Text {
                    let next = match key.code {
                        KeyCode::Left => Some(self.view.saturating_sub(1)),
                        KeyCode::Right => Some((self.view + 1).min(config::BACKLOG)),
                        _ => None,
                    };
                    if let Some(next) = next {
                        return self.switch_view(next);
                    }
                }
                if let KeyCode::Char(c @ '1'..='9') = key.code
                    && key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                {
                    return self.switch_view(c as usize - '1' as usize);
                }
                if let KeyCode::F(n) = key.code
                    && (1..=config::VIEW_NAMES.len() as u8).contains(&n)
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
                    KeyCode::Char('x') => return self.clear_filters(),
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
                        self.search_changed();
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
                    KeyCode::Char('r') => return self.refresh_all(),
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
                    KeyCode::Char('a' | 't' | 'e' | 'c' | 'L' | 'm') => {
                        if let KeyCode::Char(c) = key.code {
                            self.action_dialog(c)
                        }
                    }
                    KeyCode::Char('i') if !self.inline_slots.is_empty() => {
                        self.focus = Focus::Ticket;
                        self.scroll = self.inline_slots[0].row.saturating_sub(1);
                        self.inline_images
                            .retain(|_, s| !matches!(s, InlineImageState::Failed(_)));
                    }
                    KeyCode::Char('i' | 'I') => {
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
                    if let Some(Dialog::Filters {
                        choices,
                        selected,
                        active,
                        checked,
                        custom,
                    }) = &mut self.dialog
                    {
                        if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
                            if contains(self.filter_apply_rect, mouse.column, mouse.row) {
                                return self.dialog_key(KeyEvent::new(
                                    KeyCode::Char('s'),
                                    KeyModifiers::CONTROL,
                                ));
                            }
                            if contains(self.filter_reset_rect, mouse.column, mouse.row) {
                                return self.dialog_key(KeyEvent::new(
                                    KeyCode::Char('r'),
                                    KeyModifiers::CONTROL,
                                ));
                            }
                        }
                        if custom.is_some() {
                            return vec![];
                        }
                        for column in 0..2 {
                            let rect = self.filter_list_rects[column];
                            if contains(rect, mouse.column, mouse.row) {
                                *active = column;
                                match mouse.kind {
                                    MouseEventKind::Down(MouseButton::Left) => {
                                        let index = self.filter_list_starts[column]
                                            + (mouse.row - rect.y) as usize;
                                        if index < choices[column].len() && custom.is_none() {
                                            selected[column] = index;
                                            if choices[column][index].value == "__other" {
                                                *custom = Some(FilterInput {
                                                    column,
                                                    value: String::new(),
                                                    cursor: 0,
                                                });
                                            } else {
                                                checked[column]
                                                    .toggle(&choices[column][index].value);
                                            }
                                        }
                                    }
                                    MouseEventKind::ScrollUp => {
                                        return self.dialog_key(KeyEvent::new(
                                            KeyCode::Up,
                                            KeyModifiers::NONE,
                                        ));
                                    }
                                    MouseEventKind::ScrollDown => {
                                        return self.dialog_key(KeyEvent::new(
                                            KeyCode::Down,
                                            KeyModifiers::NONE,
                                        ));
                                    }
                                    _ => {}
                                }
                            }
                        }
                        return vec![];
                    }
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
                            self.search_changed();
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
                                if key == 'r' {
                                    return self.refresh_all();
                                }
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
                            && mouse.column > self.tickets_rect.x
                            && mouse.column < self.tickets_rect.right() - 1
                            && mouse.row > self.tickets_rect.y + 1
                            && mouse.row < self.tickets_rect.bottom() - 1
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
                                self.select_current();
                                if let Some(url) = self.items[self.visible[n]]
                                    .web_url()
                                    .filter(|u| crate::clipboard::validate(u).is_ok())
                                {
                                    self.copy_generation += 1;
                                    return vec![Job::CopyLink(crate::clipboard::Request {
                                        generation: self.copy_generation,
                                        url,
                                    })];
                                }
                                self.notice("Ticket has no safe link to copy", true);
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
        if let Some(workspace) = &self.workspace {
            return workspace.items.clone();
        }
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
        if !self.assignee_filter.is_empty() {
            return self.assignee_filter.single();
        }
        match self.view {
            config::MINE => "@me",
            config::AVAILABLE => "unassigned",
            _ => self.assignee_filter.single(),
        }
    }
    pub fn assignee_label(&self) -> String {
        if self.assignee_filter.len() > 1 {
            return format!("{} selected", self.assignee_filter.len());
        }
        match self.assignee_filter_value() {
            "@me" => "Me".into(),
            "unassigned" => String::new(),
            "" => "All".into(),
            value => self
                .workspace
                .as_ref()
                .map(|w| w.items.as_slice())
                .unwrap_or(&self.items)
                .iter()
                .map(|t| t.user("assignee"))
                .find(|u| {
                    [&u.id, &u.email, &u.login]
                        .iter()
                        .any(|s| s.eq_ignore_ascii_case(value))
                })
                .map(|u| u.label())
                .unwrap_or_else(|| value.into()),
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
        self.assignee_filter = value.into();
    }
    fn filters_dialog(&mut self) {
        let choices = [self.assignee_choices(true), self.status_choices(true).0];
        let checked = [
            if self.assignee_filter.is_empty() {
                self.assignee_filter_value().into()
            } else {
                self.assignee_filter.clone()
            },
            self.status_filter.clone(),
        ];
        let selected = [0, 1].map(|column| {
            choices[column]
                .iter()
                .position(|c| checked[column].contains(&c.value))
                .unwrap_or(0)
        });
        self.dialog = Some(Dialog::Filters {
            choices,
            selected,
            checked,
            active: 1,
            custom: None,
        });
    }
    fn refresh_filter_lists(&mut self) {
        let Some(Dialog::Filters {
            choices,
            selected,
            checked,
            active,
            custom,
        }) = &self.dialog
        else {
            return;
        };
        let cursors = [
            choices[0][selected[0]].clone(),
            choices[1][selected[1]].clone(),
        ];
        let checked = checked.clone();
        let active = *active;
        let custom = custom.clone();
        let mut next = [self.assignee_choices(true), self.status_choices(true).0];
        let mut selected = [0; 2];
        for column in 0..2 {
            for value in checked[column].values() {
                if !next[column].iter().any(|c| c.value == *value) {
                    let index = next[column].len().saturating_sub(1);
                    next[column].insert(
                        index,
                        Choice {
                            label: format!("{value} (selected)"),
                            value: value.clone(),
                            target: None,
                        },
                    );
                }
            }
            selected[column] = if let Some(index) = next[column]
                .iter()
                .position(|c| c.value == cursors[column].value)
            {
                index
            } else {
                let index = next[column].len().saturating_sub(1);
                next[column].insert(index, cursors[column].clone());
                index
            };
        }
        self.dialog = Some(Dialog::Filters {
            choices: next,
            selected,
            checked,
            active,
            custom,
        });
    }
    fn apply_filter_selections(&mut self, checked: [Selection; 2]) -> Vec<Job> {
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
        [self.assignee_filter, self.status_filter] = checked;
        // Filters intersect the selected preset/membership view, never switch it.
        self.reload()
    }
    fn assignee_choices(&self, filter: bool) -> Vec<Choice> {
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
        let fixed = if filter { 3 } else { 2 };
        if filter {
            for value in self.assignee_filter.values() {
                if !choices.iter().any(|c| c.value == *value) {
                    choices.push(Choice {
                        label: format!("{value} (current)"),
                        value: value.clone(),
                        target: None,
                    });
                }
            }
        }
        choices[fixed..].sort_by_cached_key(|c| c.label.to_lowercase());
        choices.push(Choice {
            label: "Other email / account ID…".into(),
            value: "__other".into(),
            target: None,
        });
        choices
    }
    fn assignee_picker(&mut self, filter: bool) {
        if filter {
            self.filters_dialog();
            if let Some(Dialog::Filters { active, .. }) = &mut self.dialog {
                *active = 0;
            }
            return;
        }
        let choices = self.assignee_choices(filter);
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
    fn status_choices(&self, filter: bool) -> (Vec<Choice>, bool) {
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
        if filter {
            for value in self.status_filter.values() {
                if !choices.iter().any(|c| c.value == *value) {
                    choices.push(Choice {
                        label: format!("{value} (current)"),
                        value: value.clone(),
                        target: None,
                    });
                }
            }
        }
        choices.push(Choice {
            label: "Other status…".into(),
            value: "__other".into(),
            target: None,
        });
        (choices, suggestions)
    }
    fn status_picker(&mut self, filter: bool) {
        if filter {
            self.filters_dialog();
            if let Some(Dialog::Filters { active, .. }) = &mut self.dialog {
                *active = 1;
            }
            return;
        }
        let (choices, suggestions) = self.status_choices(filter);
        let selected = 0;
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
            selected,
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
            'm' => {
                if ticket.is_epic() {
                    self.notice("Epic containers: Board/Backlog movement is disabled", true);
                    return;
                }
                let Some(id) = self.settings.board_id else {
                    self.notice(
                        "Configure --board <id> for exact membership and moves",
                        true,
                    );
                    return;
                };
                let Some(board) = self.workspace.as_ref().and_then(|w| w.board.as_ref()) else {
                    self.notice(
                        "Load verified Board membership before moving a ticket",
                        true,
                    );
                    return;
                };
                if self
                    .workspace
                    .as_ref()
                    .is_none_or(|w| !w.items.iter().any(|t| t.key == ticket.key))
                {
                    self.notice("Ticket is outside the loaded Board/Backlog scope", true);
                    return;
                }
                let kind = if board.contains(&ticket.key) {
                    "backlog"
                } else {
                    "board"
                };
                self.confirm(Action::new(kind, &ticket.key, &id.to_string()));
            }
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
                title: "Links & attachments · internal tickets / browser / image preview".into(),
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
            if attachment.is_image()
                && let Some(path) = config::local_attachment(
                    &self.attachment_dir,
                    &self.detail_key,
                    &attachment.name,
                )
            {
                return self.preview_job(path);
            }
            if attachment.is_image() {
                if let Err(e) = crate::images::validate_attachment(attachment) {
                    self.notice(e.to_string(), true);
                    return vec![];
                }
                self.image_generation += 1;
                self.awaiting_image = true;
                self.notice("Downloading image (read-only)…", false);
                return vec![Job::AttachmentImage {
                    generation: self.image_generation,
                    key: self.detail_key.clone(),
                    attachment: attachment.clone(),
                    cache: crate::images::cache_path(
                        self.cache.directory.as_deref(),
                        &self.detail_key,
                        &attachment.id,
                    ),
                    size: (
                        (self.tickets_rect.width + self.ticket_rect.width)
                            .saturating_sub(10)
                            .max(10),
                        self.ticket_rect.height.saturating_sub(4).max(6),
                    ),
                    inline: false,
                    cancel: self.preview_cancel.subscribe(),
                }];
            }
            self.notice("Non-raster attachments open in Jira", false)
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
        if key.code == KeyCode::Esc {
            if let Dialog::Filters { custom, .. } = &mut dialog
                && custom.is_some()
            {
                *custom = None;
                self.dialog = Some(dialog);
            }
            return vec![];
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return vec![];
        }
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && key.code == KeyCode::Char('r')
            && let Dialog::Filters {
                selected,
                checked,
                custom,
                ..
            } = &mut dialog
        {
            *selected = [0; 2];
            *checked = [Selection::default(), Selection::default()];
            *custom = None;
            self.dialog = Some(dialog);
            return vec![];
        }
        if let Dialog::Filters {
            choices,
            selected,
            checked,
            custom: Some(input),
            ..
        } = &mut dialog
        {
            let apply =
                key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s');
            if key.code == KeyCode::Enter || apply {
                let column = input.column;
                let value = input.value.trim().to_owned();
                if !value.is_empty() && value != "__other" {
                    let index = if let Some(index) =
                        choices[column].iter().position(|c| c.value == value)
                    {
                        index
                    } else {
                        let index = choices[column].len().saturating_sub(1);
                        choices[column].insert(
                            index,
                            Choice {
                                label: value.clone(),
                                value: value.clone(),
                                target: None,
                            },
                        );
                        index
                    };
                    selected[column] = index;
                    checked[column].insert(value);
                }
                if apply {
                    return self.apply_filter_selections(checked.clone());
                }
                if let Dialog::Filters { custom, .. } = &mut dialog {
                    *custom = None;
                }
            } else {
                crate::input::key(&mut input.value, &mut input.cursor, key);
            }
            self.dialog = Some(dialog);
            return vec![];
        }
        match &mut dialog {
            Dialog::Filters {
                choices,
                selected,
                active,
                checked,
                custom,
            } => match key.code {
                KeyCode::Tab | KeyCode::BackTab => *active = 1 - *active,
                KeyCode::Left => *active = 1,
                KeyCode::Right => *active = 0,
                KeyCode::Up | KeyCode::Char('k') => {
                    selected[*active] = selected[*active].saturating_sub(1)
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    selected[*active] =
                        (selected[*active] + 1).min(choices[*active].len().saturating_sub(1))
                }
                KeyCode::PageUp => selected[*active] = selected[*active].saturating_sub(10),
                KeyCode::PageDown => {
                    selected[*active] =
                        (selected[*active] + 10).min(choices[*active].len().saturating_sub(1))
                }
                KeyCode::Home => selected[*active] = 0,
                KeyCode::End => selected[*active] = choices[*active].len().saturating_sub(1),
                KeyCode::Char(' ') => {
                    let value = &choices[*active][selected[*active]].value;
                    if value == "__other" {
                        *custom = Some(FilterInput {
                            column: *active,
                            value: String::new(),
                            cursor: 0,
                        });
                    } else {
                        checked[*active].toggle(value);
                    }
                }
                KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    *selected = [0; 2];
                    *checked = [Selection::default(), Selection::default()];
                }
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    selected[*active] = 0;
                    checked[*active].clear();
                }
                KeyCode::Enter | KeyCode::Char('s')
                    if key.code == KeyCode::Enter
                        || key.modifiers.contains(KeyModifiers::CONTROL) =>
                {
                    if key.code == KeyCode::Enter
                        && choices[*active][selected[*active]].value == "__other"
                    {
                        *custom = Some(FilterInput {
                            column: *active,
                            value: String::new(),
                            cursor: 0,
                        });
                    } else {
                        return self.apply_filter_selections(checked.clone());
                    }
                }
                _ => {}
            },
            Dialog::Confirm(action) => {
                if key.code == KeyCode::Char('y') {
                    if self.writing {
                        self.notice("Another update is in flight", true);
                        return vec![];
                    }
                    if self.read_only {
                        self.notice("Read-only update blocked", true);
                        return vec![];
                    };
                    if matches!(action.kind.as_str(), "board" | "backlog")
                        && self
                            .workspace
                            .as_ref()
                            .is_some_and(|w| w.ticket(&action.key).is_some_and(Ticket::is_epic))
                    {
                        self.notice(
                            "Epic containers cannot be moved between Board and Backlog",
                            true,
                        );
                        return vec![];
                    }
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
                            self.status_filter = choice.value.into();
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
                                && !t.is_some_and(|t| t.fields.get("description").is_some())
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
pub const HELP: &str = "LAYOUT\nTickets | Ticket. Tab changes focus. Click a ticket row to select it and copy its Jira link. Mouse opens detail links. Unassigned person fields are blank.\n\nFOUR PREDEFINED VIEWS\nLeft/Right arrows switch views (except while editing text).\nCtrl+1 Mine: assigned to you.\nCtrl+2 Available: unassigned.\nCtrl+3 Board: actual Board membership, including completed rows.\nCtrl+4 Backlog: actual Backlog membership.\nMine and Available cover both sections, Board first, Rank within each.\nClick tabs or use Alt+1…4 / F1…F4.\n--board <id> enables the opt-in Jira Software API; no guessed status mapping.\n\nSEARCH\nType digits: project prefix inferred, lookup automatic across the scope.\n/ instantly searches the locally cached view, including descriptions.\nEnter submits immediately; Esc clears search/restores the view.\n\nTICKETS\n↑↓ / j k navigate; Enter focuses details; Esc returns from linked ticket.\na assign · t status · e edit · c comment · L link tickets\nm move ONE ticket between Board and Backlog (y confirms)\nl links/attachments · O opens Jira · C loads all comments\n\nFILTERS\nf shows multi-select assignee + status lists from the shared cache.\nSpace / click toggles checks; ↑↓ moves cursor; Tab / ←→ changes column.\nEnter / Ctrl+S applies both lists; Esc cancels all staged changes.\nStatus first, then Assignee. OR within lists, AND between lists and text.\nEach view remembers only its own filters/search; returning restores them.\nAll/Any clears one list; Ctrl+U clears active; Ctrl+R resets both (Apply commits).\nA / T or dropdowns open the same popup. Other adds an exact value without replacing checks.\ng / Ctrl+G opens optional advanced project/JQL settings.\nF6 / browsing x clears only this view's filters/search; other views stay unchanged.\n0 shown with N in the view means filters hide those tickets, not lost refresh data.\nEpics have purple row/title highlights; / epic searches their type. SP shows story points (— unknown/unset; no child-ticket sum). Status badges are visual only. Done ✓ / Discarded ✕; full names remain in workflow/filter actions.\n\nATTACHMENTS\nPNG/JPEG/GIF/WebP images load inline, read-only, at the ticket pane width. Tall images scroll vertically; l opens a tab-wide preview.\ni jumps to images/retries; l opens a larger preview; I chooses a local file.\nNo external media or redirects; authenticated downloads are bounded and account-pinned.\n--no-inline-images disables automatic loading.\nKitty/other native graphics where supported, otherwise Unicode half-blocks.\n\nSAFETY\nUpdates target one ticket and need y confirmation. n/Esc cancels.\n--read-only blocks changes. ! shows the full CLI error.\nF5 / Ctrl+R / r fully reload the scope and revalidate all scoped details. Cached data stays visible; tab/filters/order/selection are retained. Details warm with bounded concurrency. In Filters, Ctrl+R still resets pending filters.\nq quits (wait for writes to finish).";

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> App {
        let mut settings = Settings {
            project: "DEV".into(),
            last_view: config::BACKLOG,
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
        a.workspace = Some(Workspace {
            items: data::demo_tickets(),
            mine: HashSet::from(["DEV-101".into()]),
            complete: true,
            synced_at: config::now(),
            ..Default::default()
        });
        a.workspace.as_mut().unwrap().reindex();
        a.last_query = a.settings.query(config::BACKLOG, "", "", "").unwrap();
        a.apply_local();
        a.prefetch.clear();
        for t in data::demo_tickets() {
            a.details.insert(t.key.clone(), (t, config::now()));
        }
        a
    }
    fn key(c: char, mods: KeyModifiers) -> Event {
        Event::Key(KeyEvent::new(KeyCode::Char(c), mods))
    }
    #[test]
    fn ticket_row_click_copies_the_clicked_link_but_other_navigation_does_not() {
        use crossterm::event::MouseEvent;
        for height in [1, 2] {
            let mut a = app();
            a.read_only = true;
            a.tickets_rect = Rect::new(0, 3, 80, 16);
            a.row_height = height;
            a.table = TableState::default().with_offset(1);
            let mouse = |kind, x, y| {
                Event::Mouse(MouseEvent {
                    kind,
                    column: x,
                    row: y,
                    modifiers: KeyModifiers::NONE,
                })
            };
            let expected = a.items[a.visible[1]].web_url().unwrap();
            let jobs = a.event(mouse(
                MouseEventKind::Down(MouseButton::Left),
                4,
                5 + height - 1,
            ));
            assert_eq!(a.selected, 1);
            assert_eq!(jobs.len(), 1);
            let Job::CopyLink(request) = &jobs[0] else {
                panic!("Expected link copy")
            };
            assert_eq!(request.url, expected);
            assert_eq!(request.generation, 1);
            let jobs = a.event(mouse(MouseEventKind::Down(MouseButton::Left), 4, 5));
            assert!(matches!(&jobs[..], [Job::CopyLink(_)]));
            assert_eq!(a.copy_generation, 2);
            for (x, y) in [(4, 4), (0, 5), (79, 5), (4, 18), (4, 16)] {
                assert!(
                    !a.event(mouse(MouseEventKind::Down(MouseButton::Left), x, y))
                        .iter()
                        .any(|j| matches!(j, Job::CopyLink(_)))
                );
            }
            assert!(
                !a.event(mouse(MouseEventKind::ScrollDown, 4, 5))
                    .iter()
                    .any(|j| matches!(j, Job::CopyLink(_)))
            );
            assert!(
                !a.event(Event::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)))
                    .iter()
                    .any(|j| matches!(j, Job::CopyLink(_)))
            );
            a.filters_dialog();
            assert!(
                !a.event(mouse(MouseEventKind::Down(MouseButton::Left), 4, 5))
                    .iter()
                    .any(|j| matches!(j, Job::CopyLink(_)))
            );
            assert_eq!(a.copy_generation, 2);
        }
    }
    #[test]
    fn unsafe_ticket_links_are_not_copied_and_old_clipboard_notices_are_ignored() {
        use crossterm::event::MouseEvent;
        let mut a = app();
        a.tickets_rect = Rect::new(0, 3, 80, 16);
        a.items[a.visible[0]].self_url = "javascript:alert(1)".into();
        let jobs = a.event(Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 4,
            row: 5,
            modifiers: KeyModifiers::NONE,
        }));
        assert!(jobs.is_empty());
        assert_eq!(a.copy_generation, 0);
        a.copy_generation = 2;
        a.status = "unchanged".into();
        a.message(Message::Copied(crate::clipboard::Completion {
            request: crate::clipboard::Request {
                generation: 1,
                url: "https://example.invalid/browse/DEV-1".into(),
            },
            result: Ok(crate::clipboard::Method::Native),
        }));
        assert_eq!(a.status, "unchanged");
    }
    #[test]
    fn numbers_search_not_views() {
        let mut a = app();
        a.event(key('1', KeyModifiers::NONE));
        assert_eq!(a.search_mode as u8, SearchMode::Number as u8);
        assert_eq!(a.search, "1");
        assert_eq!(a.view, config::BACKLOG);
        a.event(key('0', KeyModifiers::NONE));
        a.event(key('1', KeyModifiers::NONE));
        assert_eq!(a.search, "101");
        a.search_due = Some(Instant::now());
        let jobs = a.advance();
        assert!(jobs.is_empty());
        assert_eq!(a.detail_key, "DEV-101");
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
            revision: 0,
            background: false,
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
        assert!(jobs.is_empty());
        assert_eq!(a.detail_key, "DEV-101");
        a.message(Message::Detail {
            key: "DEV-101".into(),
            lookup: Some(a.lookup_generation),
            revision: 0,
            background: false,
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
            revision: 0,
            background: false,
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
        assert_eq!(
            a.view_names(),
            vec!["Mine", "Available", "Board", "Backlog"]
        );
        for (keycode, view) in [
            ('1', config::MINE),
            ('2', config::AVAILABLE),
            ('3', config::BOARD),
            ('4', config::BACKLOG),
        ] {
            let jobs = a.event(key(keycode, KeyModifiers::CONTROL));
            assert_eq!(a.view, view);
            assert!(a.dialog.is_none());
            assert!(jobs.is_empty());
        }
    }
    #[test]
    fn fresh_estimate_metadata_upgrades_old_cached_details_without_waiting_for_cli_warming() {
        let mut a = app();
        let key = a.detail_key.clone();
        a.details.get_mut(&key).unwrap().0.estimate_fields.clear();
        assert_eq!(a.details[&key].0.points_label(), "—");
        let mut items = data::demo_tickets();
        items.iter_mut().find(|t| t.key == key).unwrap().fields["customfield_90123"] =
            serde_json::json!(5);
        a.message(Message::Synced {
            generation: a.generation,
            result: Ok((
                Workspace {
                    items,
                    complete: true,
                    ..Default::default()
                },
                vec![],
            )),
        });
        assert_eq!(a.details[&key].0.points_label(), "5");
        assert_eq!(
            a.workspace
                .as_ref()
                .unwrap()
                .ticket(&key)
                .unwrap()
                .points_label(),
            "5"
        );
    }
    #[test]
    fn inline_images_are_viewport_bounded_cancelled_on_ticket_change_and_stale_results_are_ignored()
    {
        let mut a = app();
        let key = a.detail_key.clone();
        a.details.get_mut(&key).unwrap().0.fields["attachment"] = serde_json::json!([
            {"id":"23","filename":"a.png","mimeType":"image/png","size":10},
            {"id":"24","filename":"b.png","mimeType":"image/png","size":10},
            {"id":"25","filename":"c.png","mimeType":"image/png","size":10}]);
        a.ticket_rect = Rect::new(0, 0, 80, 40);
        a.document(78);
        assert_eq!(a.inline_slots.len(), 3);
        a.scroll = a.inline_slots.last().unwrap().row + 10;
        assert!(a.inline_jobs().is_empty());
        a.scroll = a.inline_slots[0].row;
        let jobs = a.inline_jobs();
        assert_eq!(jobs.len(), 2);
        assert!(a.inline_jobs().is_empty());
        let Job::AttachmentImage {
            generation, cancel, ..
        } = &jobs[0]
        else {
            panic!()
        };
        let generation = *generation;
        let cancel = cancel.clone();
        a.detail_key = "DEV-103".into();
        a.document(78);
        assert!(*cancel.borrow());
        assert!(a.inline_images.is_empty());
        a.message(Message::InlineImage {
            generation,
            key,
            id: "23".into(),
            result: Err(anyhow::anyhow!("stale failure")),
        });
        assert!(a.inline_images.is_empty());
    }
    #[test]
    fn image_failures_do_not_hide_tickets_and_no_images_disables_automatic_jobs() {
        let mut a = app();
        a.inline_key = a.detail_key.clone();
        a.inline_images
            .insert("23".into(), InlineImageState::Pending);
        let n = a.visible.len();
        a.message(Message::InlineImage {
            generation: a.inline_generation,
            key: a.detail_key.clone(),
            id: "23".into(),
            result: Err(anyhow::anyhow!("HTTP 403")),
        });
        assert_eq!(a.visible.len(), n);
        assert!(matches!(a.inline_images["23"], InlineImageState::Failed(_)));
        a.automatic_images = false;
        a.doc_dirty = true;
        a.document(50);
        assert!(a.inline_slots.is_empty());
        assert!(a.inline_jobs().is_empty());
    }
    #[test]
    fn palette_cache_keeps_status_colors_identical_after_restart() {
        let dir = tempfile::tempdir().unwrap();
        let mut a = app();
        a.cache = Cache {
            directory: Some(dir.path().into()),
        };
        let w = a.workspace.clone().unwrap();
        a.message(Message::Synced {
            generation: a.generation,
            result: Ok((w, vec![])),
        });
        let restarted = App::new(
            a.backend.clone(),
            a.cache.clone(),
            a.settings.clone(),
            true,
            500,
            PathBuf::new(),
        );
        for t in &a.items {
            assert_eq!(a.status_colors.style(t), restarted.status_colors.style(t));
        }
        assert!(restarted.status_colors.valid());
    }
    #[test]
    fn changing_scope_resets_all_views_filters_without_changing_palette() {
        let mut a = app();
        a.status_filter = "To Do".into();
        a.switch_view(config::BOARD);
        a.assignee_filter = "sam".into();
        a.remote_search = "words".into();
        a.settings.project = "OTHER".into();
        let jobs = a.reload();
        assert!(matches!(&jobs[..], [Job::Sync { .. }]));
        assert!(a.assignee_filter.is_empty());
        assert!(a.remote_search.is_empty());
        assert!(
            a.view_filters
                .iter()
                .all(|v| v.assignee.is_empty() && v.status.is_empty() && v.text.is_empty())
        );
    }
    #[test]
    fn assignee_status_and_text_filters_are_independent_for_each_view() {
        let mut a = app();
        a.workspace.as_mut().unwrap().board = Some(["DEV-101".into(), "DEV-103".into()].into());
        a.workspace.as_mut().unwrap().reindex();
        a.items_are_workspace = false;
        for (view, assignees, statuses, text, key) in [
            (
                config::BACKLOG,
                vec!["unassigned"],
                vec!["Refinement"],
                "Enter",
                "DEV-102",
            ),
            (
                config::BOARD,
                vec!["@me", "sam"],
                vec!["In Progress", "To Do"],
                "confirmation",
                "DEV-103",
            ),
            (
                config::AVAILABLE,
                vec!["unassigned"],
                vec!["Refinement"],
                "Enter",
                "DEV-102",
            ),
            (
                config::MINE,
                vec!["@me"],
                vec!["In Progress"],
                "fast",
                "DEV-101",
            ),
        ] {
            assert!(a.switch_view(view).is_empty());
            assert!(a.status_filter.is_empty());
            assert!(a.assignee_filter.is_empty());
            assert!(a.remote_search.is_empty());
            a.assignee_filter = assignees
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
                .into();
            a.status_filter = statuses
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
                .into();
            a.remote_search = text.into();
            a.reload();
            assert_eq!(a.selected_ticket().unwrap().key, key);
        }
        a.switch_view(config::BACKLOG);
        assert_eq!(a.status_filter, "Refinement");
        assert_eq!(a.remote_search, "Enter");
        assert_eq!(a.assignee_filter, "unassigned");
        assert!(a.clear_filters().is_empty());
        a.switch_view(config::BOARD);
        assert_eq!(a.assignee_filter.len(), 2);
        assert_eq!(a.status_filter.len(), 2);
        assert_eq!(a.remote_search, "confirmation");
        let w = a.workspace.clone().unwrap();
        a.refresh_all();
        a.message(Message::Synced {
            generation: a.generation,
            result: Ok((w, vec![])),
        });
        assert_eq!(a.status_filter.len(), 2);
        assert_eq!(a.selected_ticket().unwrap().key, "DEV-103");
        a.switch_view(config::BACKLOG);
        assert!(a.status_filter.is_empty());
        assert!(a.remote_search.is_empty());
        assert!(a.assignee_filter.is_empty());
        a.switch_view(config::AVAILABLE);
        assert_eq!(a.status_filter, "Refinement");
        assert_eq!(a.remote_search, "Enter");
    }
    #[test]
    fn switching_during_text_input_saves_only_that_views_search() {
        let mut a = app();
        a.event(key('/', KeyModifiers::NONE));
        a.event(Event::Paste("staged words".into()));
        a.switch_view(config::BOARD);
        assert!(a.remote_search.is_empty());
        a.switch_view(config::BACKLOG);
        assert_eq!(a.remote_search, "staged words");
        a.event(Event::Paste("102".into()));
        a.switch_view(config::MINE);
        assert!(!a.lookup_active);
        a.switch_view(config::BACKLOG);
        assert_eq!(a.remote_search, "staged words");
        assert!(!a.lookup_active);
    }
    #[test]
    fn filter_panel_starts_with_status_and_arrow_navigation_matches_visual_order() {
        let mut a = app();
        a.filters_dialog();
        assert!(matches!(a.dialog, Some(Dialog::Filters { active: 1, .. })));
        a.event(Event::Key(KeyEvent::new(
            KeyCode::Right,
            KeyModifiers::NONE,
        )));
        assert!(matches!(a.dialog, Some(Dialog::Filters { active: 0, .. })));
        a.event(Event::Key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)));
        assert!(matches!(a.dialog, Some(Dialog::Filters { active: 1, .. })));
    }
    #[test]
    fn text_search_stays_in_its_assignment_view() {
        let mut a = app();
        a.view = config::AVAILABLE;
        a.event(key('/', KeyModifiers::NONE));
        for c in "Enter".chars() {
            a.event(key(c, KeyModifiers::NONE));
        }
        a.search_due = Some(Instant::now());
        let jobs = a.advance();
        assert_eq!(a.view, config::AVAILABLE);
        assert!(jobs.is_empty());
        assert_eq!(a.visible.len(), 1);
        assert_eq!(a.selected_ticket().unwrap().key, "DEV-102");
        a.apply_assignee_filter("@me".into());
        assert_eq!(a.view, config::AVAILABLE);
        assert!(a.query().unwrap().contains("currentUser()"));
        a.apply_assignee_filter("sam".into());
        assert_eq!(a.view, config::AVAILABLE);
        assert!(a.query().unwrap().contains("assignee = \"sam\""));
        a.switch_view(config::MINE);
        assert!(a.remote_search.is_empty());
        assert!(a.assignee_filter.is_empty());
        assert!(a.query().unwrap().contains("currentUser()"));
        a.switch_view(config::AVAILABLE);
        assert_eq!(a.remote_search, "Enter");
        assert_eq!(a.assignee_filter, "sam");
    }
    #[test]
    fn arrows_switch_views_without_queries_or_stealing_editor_keys() {
        let mut a = app();
        for (code, view) in [
            (KeyCode::Left, config::BOARD),
            (KeyCode::Left, config::AVAILABLE),
            (KeyCode::Left, config::MINE),
            (KeyCode::Left, config::MINE),
            (KeyCode::Right, config::AVAILABLE),
            (KeyCode::Right, config::BOARD),
            (KeyCode::Right, config::BACKLOG),
            (KeyCode::Right, config::BACKLOG),
        ] {
            assert!(
                a.event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
                    .is_empty()
            );
            assert_eq!(a.view, view);
        }
        a.action_dialog('c');
        a.event(Event::Key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)));
        assert_eq!(a.view, config::BACKLOG);
        assert!(matches!(a.dialog, Some(Dialog::Editor { .. })));
        a.dialog = None;
        a.event(key('/', KeyModifiers::NONE));
        a.event(Event::Key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)));
        assert_eq!(a.view, config::BACKLOG);
        assert!(matches!(a.search_mode, SearchMode::Text));
    }
    #[test]
    fn filters_keep_server_backlog_order_instead_of_sorting_by_key() {
        let mut a = app();
        let mut tickets = data::demo_tickets();
        tickets.reverse();
        a.workspace.as_mut().unwrap().items = tickets;
        a.workspace.as_mut().unwrap().reindex();
        a.items_are_workspace = false;
        a.reload();
        assert_eq!(
            a.visible
                .iter()
                .map(|i| a.items[*i].key.as_str())
                .collect::<Vec<_>>(),
            ["DEV-103", "DEV-102", "DEV-101"]
        );
        a.apply_assignee_filter("sam".into());
        a.reload();
        assert_eq!(a.selected_ticket().unwrap().key, "DEV-103");
        a.switch_view(config::BACKLOG);
        assert_eq!(a.items[0].key, "DEV-103");
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
    fn startup_reads_one_shared_snapshot_before_any_network_reply() {
        let dir = tempfile::tempdir().unwrap();
        let mut a = app();
        a.cache = Cache {
            directory: Some(dir.path().into()),
        };
        a.cache
            .put(&a.workspace_key(), a.workspace.as_ref().unwrap())
            .unwrap();
        let mut next = App::new(
            a.backend.clone(),
            a.cache.clone(),
            a.settings.clone(),
            true,
            500,
            PathBuf::new(),
        );
        let jobs = next.reload();
        assert_eq!(next.items.len(), 3);
        assert_eq!(next.visible.len(), 3);
        assert!(matches!(jobs.first(), Some(Job::Sync { .. })));
        next.switch_view(config::MINE);
        assert_eq!(next.visible.len(), 1);
        next.switch_view(config::AVAILABLE);
        assert_eq!(next.visible.len(), 1);
        assert!(next.reload().is_empty());
        assert!(next.syncing);
    }
    #[test]
    fn failed_background_sync_does_not_clear_the_local_dataset() {
        let mut a = app();
        a.syncing = true;
        a.message(Message::Synced {
            generation: a.generation,
            result: Err(anyhow::anyhow!("offline")),
        });
        assert_eq!(a.items.len(), 3);
        assert_eq!(a.visible.len(), 3);
        assert!(!a.syncing);
        a.switch_view(config::AVAILABLE);
        assert_eq!(a.selected_ticket().unwrap().key, "DEV-102");
    }
    #[test]
    fn refresh_preserves_selection_and_scroll_and_ignores_old_epochs() {
        let mut a = app();
        a.focus = Focus::Ticket;
        a.scroll = 3;
        let key = a.detail_key.clone();
        let w = a.workspace.clone().unwrap();
        a.message(Message::Synced {
            generation: a.generation,
            result: Ok((w, vec![])),
        });
        assert_eq!(a.detail_key, key);
        assert_eq!(a.scroll, 3);
        a.generation += 1;
        a.message(Message::Synced {
            generation: a.generation - 1,
            result: Ok((Workspace::default(), vec![])),
        });
        assert_eq!(a.items.len(), 3);
    }
    #[test]
    fn background_prefetch_is_bounded_and_stale_details_cannot_undo_writes() {
        let mut a = app();
        a.selection_due = None;
        a.prefetch = a.items.iter().map(|t| t.key.clone()).collect();
        let jobs = a.advance();
        assert_eq!(a.background.len(), 2);
        assert_eq!(
            jobs.iter()
                .filter(|j| matches!(
                    j,
                    Job::Detail {
                        background: true,
                        ..
                    }
                ))
                .count(),
            2
        );
        assert!(a.advance().is_empty());
        let mut old = data::demo_tickets().remove(0);
        old.fields["summary"] = serde_json::json!("old reply");
        a.detail_revisions.insert("DEV-101".into(), 1);
        a.message(Message::Detail {
            key: "DEV-101".into(),
            lookup: None,
            revision: 0,
            background: true,
            result: Ok(old),
        });
        assert_ne!(a.details["DEV-101"].0.summary(), "old reply");
        assert_eq!(a.background.len(), 1);
    }
    #[test]
    fn descriptions_are_usable_without_waiting_for_full_detail_cli_calls() {
        let mut a = app();
        a.details.clear();
        a.document(80);
        assert!(
            a.document
                .iter()
                .flat_map(|l| &l.chunks)
                .any(|c| c.text.contains("Bold"))
        );
        assert!(
            a.document
                .iter()
                .flat_map(|l| &l.chunks)
                .any(|c| c.text.contains("ready locally"))
        );
    }
    #[test]
    fn full_refresh_shortcuts_bypass_details_and_preserve_the_view() {
        for event in [
            key('r', KeyModifiers::NONE),
            key('r', KeyModifiers::CONTROL),
            Event::Key(KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE)),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let mut a = app();
            a.read_only = true;
            a.cache = Cache {
                directory: Some(dir.path().into()),
            };
            let snapshot = a.workspace.clone().unwrap();
            for t in &snapshot.items {
                a.cache.put(&full_cache_key(&t.key), t).unwrap();
                a.full_ready.insert(t.key.clone());
            }
            a.selected = 1;
            a.table.select(Some(1));
            *a.table.offset_mut() = 1;
            a.detail_key = "DEV-102".into();
            a.scroll = 7;
            a.selection_due = None;
            let jobs = a.event(event);
            let [
                Job::Sync {
                    generation,
                    changed,
                    ..
                },
            ] = &jobs[..]
            else {
                panic!("full snapshot request missing")
            };
            assert!(changed.is_none());
            assert!(a.full_sync);
            assert_eq!(a.dirty_details.len(), 3);
            assert!(a.cache.exists(&full_cache_key("DEV-101")));
            assert_eq!(
                a.cache
                    .get::<HashSet<String>>(&a.dirty_key())
                    .unwrap()
                    .0
                    .len(),
                3
            );
            a.message(Message::Synced {
                generation: *generation,
                result: Ok((snapshot.clone(), vec![])),
            });
            assert_eq!(
                (a.view, a.selected, a.table.offset(), a.scroll),
                (config::BACKLOG, 1, 1, 7)
            );
            assert_eq!(a.detail_key, "DEV-102");
            let jobs = a.advance();
            assert!(a.background.len() <= 2);
            assert_eq!(jobs.len(), 3);
            for job in jobs {
                let Job::Detail {
                    key,
                    lookup,
                    revision,
                    background,
                } = job
                else {
                    panic!("refresh must not write")
                };
                let ticket = snapshot
                    .items
                    .iter()
                    .find(|t| t.key == key)
                    .unwrap()
                    .clone();
                a.message(Message::Detail {
                    key,
                    lookup,
                    revision,
                    background,
                    result: Ok(ticket),
                });
            }
            assert!(a.dirty_details.is_empty());
        }
    }
    #[test]
    fn full_refresh_coalesces_requests_and_keeps_cached_rows_on_failure() {
        let mut a = app();
        let snapshot = a.workspace.clone().unwrap();
        a.syncing = true;
        for _ in 0..3 {
            assert!(a.refresh_all().is_empty());
        }
        assert!(a.refresh_pending);
        let jobs = a.message(Message::Synced {
            generation: a.generation,
            result: Ok((snapshot.clone(), vec![])),
        });
        let [
            Job::Sync {
                generation,
                changed,
                ..
            },
        ] = &jobs[..]
        else {
            panic!()
        };
        assert!(changed.is_none());
        assert!(!a.refresh_pending);
        assert!(a.refresh_all().is_empty());
        assert!(!a.refresh_pending);
        a.message(Message::Synced {
            generation: *generation,
            result: Err(anyhow::anyhow!("synthetic offline failure")),
        });
        assert_eq!(
            a.items.iter().map(|t| t.key.as_str()).collect::<Vec<_>>(),
            ["DEV-101", "DEV-102", "DEV-103"]
        );
        assert_eq!(a.dirty_details.len(), 3);
        assert!(a.error.contains("cached tickets retained"));
    }
    #[test]
    fn refresh_rejects_pre_refresh_detail_responses() {
        let mut a = app();
        let key = "DEV-101".to_string();
        a.pending.insert(key.clone());
        a.background.insert((key.clone(), 0));
        a.refresh_all();
        let mut stale = data::demo_tickets().remove(0);
        stale.fields["summary"] = serde_json::json!("must not undo refresh");
        a.message(Message::Detail {
            key: key.clone(),
            lookup: None,
            revision: 0,
            background: true,
            result: Ok(stale),
        });
        assert_ne!(a.details[&key].0.summary(), "must not undo refresh");
        assert!(a.dirty_details.contains(&key));
        assert!(!a.full_ready.contains(&key));
    }
    #[test]
    fn refresh_does_not_steal_editor_or_filter_controls_and_works_during_search() {
        let mut a = app();
        a.search_mode = SearchMode::Text;
        a.search = "cached words".into();
        a.remote_search = "cached words".into();
        a.assignee_filter = "sam".into();
        a.status_filter = "To Do".into();
        let jobs = a.event(key('r', KeyModifiers::CONTROL));
        assert!(matches!(&jobs[..], [Job::Sync { .. }]));
        assert_eq!(a.search, "cached words");
        assert_eq!(a.assignee_filter, "sam");
        assert_eq!(a.status_filter, "To Do");
        let mut a = app();
        a.action_dialog('c');
        assert!(a.event(key('r', KeyModifiers::CONTROL)).is_empty());
        assert!(!a.syncing);
        assert!(matches!(a.dialog, Some(Dialog::Editor { .. })));
        let mut a = app();
        a.filters_dialog();
        assert!(a.event(key('r', KeyModifiers::CONTROL)).is_empty());
        assert!(!a.syncing);
        assert!(matches!(a.dialog, Some(Dialog::Filters { .. })));
        let mut a = app();
        a.writing = true;
        assert!(a.refresh_all().is_empty());
        assert_eq!(a.generation, 0);
        assert!(a.dirty_details.is_empty());
    }
    #[test]
    fn periodic_sync_uses_updated_overlap_and_is_not_duplicated_by_tabs() {
        let mut a = app();
        a.selection_due = None;
        a.next_sync = Some(Instant::now());
        let jobs = a.advance();
        assert!(
            matches!(jobs.first(),Some(Job::Sync{changed:Some(q),..}) if q.contains("updated >= -"))
        );
        for view in [config::MINE, config::AVAILABLE, config::BACKLOG] {
            assert!(a.switch_view(view).is_empty())
        }
        assert!(a.sync_workspace().is_empty());
    }
    #[test]
    fn filters_show_cached_lists_even_when_current_view_has_no_matches() {
        let mut a = app();
        a.switch_view(config::MINE);
        a.status_filter = "Refinement".into();
        a.reload();
        assert!(a.visible.is_empty());
        a.event(key('f', KeyModifiers::NONE));
        let Some(Dialog::Filters {
            choices, selected, ..
        }) = &a.dialog
        else {
            panic!()
        };
        assert!(choices[0].iter().any(|c| c.value == "alex"));
        assert!(choices[0].iter().any(|c| c.value == "sam"));
        for status in ["In Progress", "Refinement", "To Do"] {
            assert!(choices[1].iter().any(|c| c.value == status));
        }
        assert_eq!(choices[0][selected[0]].value, "@me");
        assert_eq!(choices[1][selected[1]].value, "Refinement");
    }
    fn toggle_filter(a: &mut App, column: usize, value: &str) {
        if let Some(Dialog::Filters {
            choices,
            selected,
            active,
            ..
        }) = &mut a.dialog
        {
            *active = column;
            selected[column] = choices[column]
                .iter()
                .position(|c| c.value == value)
                .unwrap();
        } else {
            panic!("Expected filter dialog");
        }
        assert!(a.event(key(' ', KeyModifiers::NONE)).is_empty());
    }
    #[test]
    fn multi_filters_or_within_columns_and_text_intersect_without_sorting_or_writes() {
        let mut a = app();
        a.read_only = true;
        let mut workspace = a.workspace.clone().unwrap();
        workspace.items.reverse();
        for t in &mut workspace.items {
            if t.key == "DEV-101" {
                t.fields["description"] = serde_json::json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"deepcachedtoken"}]}]});
            }
        }
        workspace.reindex();
        a.message(Message::Synced {
            generation: a.generation,
            result: Ok((workspace, vec![])),
        });
        a.filters_dialog();
        for v in ["@me", "sam"] {
            toggle_filter(&mut a, 0, v);
        }
        for v in ["In Progress", "To Do"] {
            toggle_filter(&mut a, 1, v);
        }
        assert!(a.assignee_filter.is_empty());
        assert!(a.status_filter.is_empty());
        assert!(a.event(key('s', KeyModifiers::CONTROL)).is_empty());
        assert!(!a.writing);
        assert_eq!(a.view, config::BACKLOG);
        assert_eq!(
            a.visible
                .iter()
                .map(|i| a.items[*i].key.as_str())
                .collect::<Vec<_>>(),
            ["DEV-103", "DEV-101"]
        );
        a.remote_search = "deepcachedtoken".into();
        a.reload();
        assert_eq!(a.selected_ticket().unwrap().key, "DEV-101");
        a.filters_dialog();
        let Some(Dialog::Filters { checked, .. }) = &a.dialog else {
            panic!()
        };
        assert!(checked[0].contains("@me") && checked[0].contains("sam"));
        assert!(checked[1].contains("To Do") && checked[1].contains("In Progress"));
    }
    #[test]
    fn toggling_filters_does_not_apply_until_enter_and_cancel_preserves_all_values() {
        let mut a = app();
        a.assignee_filter = vec!["sam".into(), "@me".into()].into();
        a.status_filter = vec!["To Do".into(), "In Progress".into()].into();
        a.reload();
        a.filters_dialog();
        toggle_filter(&mut a, 1, "To Do");
        toggle_filter(&mut a, 0, "sam");
        a.event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
        assert_eq!(a.assignee_filter.len(), 2);
        assert_eq!(a.status_filter.len(), 2);
        a.filters_dialog();
        toggle_filter(&mut a, 1, "");
        a.event(key('s', KeyModifiers::CONTROL));
        assert!(a.status_filter.is_empty());
        assert_eq!(a.assignee_filter.len(), 2);
    }
    #[test]
    fn multiple_checks_and_custom_input_survive_workspace_sync() {
        let mut a = app();
        a.filters_dialog();
        for v in ["@me", "sam"] {
            toggle_filter(&mut a, 0, v);
        }
        for v in ["In Progress", "To Do"] {
            toggle_filter(&mut a, 1, v);
        }
        toggle_filter(&mut a, 1, "__other");
        a.event(Event::Paste("QA, follow-up".into()));
        let mut workspace = a.workspace.clone().unwrap();
        let mut new = data::demo_tickets().remove(2);
        new.key = "DEV-104".into();
        new.fields["status"] = serde_json::json!({"name":"New status"});
        workspace.items.push(new);
        a.message(Message::Synced {
            generation: a.generation,
            result: Ok((workspace, vec![])),
        });
        let Some(Dialog::Filters {
            checked,
            custom: Some(input),
            choices,
            ..
        }) = &a.dialog
        else {
            panic!()
        };
        assert_eq!(checked[0].len(), 2);
        assert_eq!(checked[1].len(), 2);
        assert_eq!(input.value, "QA, follow-up");
        assert!(choices[1].iter().any(|c| c.value == "New status"));
        a.event(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));
        let Some(Dialog::Filters {
            checked, custom, ..
        }) = &a.dialog
        else {
            panic!()
        };
        assert!(custom.is_none());
        assert_eq!(checked[1].len(), 3);
        assert!(a.event(key('s', KeyModifiers::CONTROL)).is_empty());
        assert!(a.status_filter.contains("QA, follow-up"));
        assert_eq!(a.status_filter.len(), 3);
        assert!(!a.writing);
    }
    #[test]
    fn filters_keep_membership_view_even_when_selecting_me_or_unassigned() {
        let mut a = app();
        let mut workspace = a.workspace.clone().unwrap();
        workspace.board = Some(["DEV-101".into(), "DEV-103".into()].into());
        workspace.reindex();
        a.workspace = Some(workspace);
        a.switch_view(config::BOARD);
        a.filters_dialog();
        toggle_filter(&mut a, 0, "@me");
        a.event(key('s', KeyModifiers::CONTROL));
        assert_eq!(a.view, config::BOARD);
        assert_eq!(a.selected_ticket().unwrap().key, "DEV-101");
        a.switch_view(config::BACKLOG);
        a.filters_dialog();
        toggle_filter(&mut a, 0, "unassigned");
        a.event(key('s', KeyModifiers::CONTROL));
        assert_eq!(a.view, config::BACKLOG);
        assert_eq!(a.selected_ticket().unwrap().key, "DEV-102");
    }
    #[test]
    fn reset_while_typing_custom_filter_clears_all_staged_values_without_applying() {
        let mut a = app();
        a.status_filter = "To Do".into();
        a.filters_dialog();
        toggle_filter(&mut a, 1, "__other");
        a.event(Event::Paste("Never applied".into()));
        a.event(key('r', KeyModifiers::CONTROL));
        let Some(Dialog::Filters {
            checked,
            custom,
            selected,
            ..
        }) = &a.dialog
        else {
            panic!()
        };
        assert!(checked.iter().all(Selection::is_empty));
        assert!(custom.is_none());
        assert_eq!(*selected, [0, 0]);
        assert_eq!(a.status_filter, "To Do");
    }
    #[test]
    fn custom_filter_escape_returns_to_staged_lists_instead_of_applying() {
        let mut a = app();
        a.filters_dialog();
        toggle_filter(&mut a, 1, "To Do");
        toggle_filter(&mut a, 1, "__other");
        a.event(Event::Paste("Never applied".into()));
        a.event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
        let Some(Dialog::Filters {
            custom, checked, ..
        }) = &a.dialog
        else {
            panic!()
        };
        assert!(custom.is_none());
        assert!(checked[1].contains("To Do"));
        assert!(a.status_filter.is_empty());
    }
    #[test]
    fn list_filters_stage_both_columns_and_apply_without_jira_writes() {
        let mut a = app();
        a.read_only = true;
        a.assignee_picker(true);
        for _ in 0..4 {
            a.event(Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)));
        }
        a.event(key(' ', KeyModifiers::NONE));
        a.event(Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)));
        for _ in 0..3 {
            a.event(Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)));
        }
        a.event(key(' ', KeyModifiers::NONE));
        assert!(a.assignee_filter.is_empty());
        assert!(a.status_filter.is_empty());
        let jobs = a.event(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));
        assert!(jobs.is_empty());
        assert!(!a.writing);
        assert_eq!(a.assignee_filter, "sam");
        assert_eq!(a.status_filter, "To Do");
        assert_eq!(a.selected_ticket().unwrap().key, "DEV-103");
        assert!(a.dialog.is_none());
    }
    #[test]
    fn filter_cancel_and_reset_do_not_apply_until_confirmed() {
        let mut a = app();
        a.switch_view(config::MINE);
        a.status_filter = "In Progress".into();
        a.reload();
        a.filters_dialog();
        a.event(key('r', KeyModifiers::CONTROL));
        a.event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
        assert_eq!(a.view, config::MINE);
        assert_eq!(a.status_filter, "In Progress");
        a.filters_dialog();
        a.event(key('r', KeyModifiers::CONTROL));
        a.event(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));
        assert_eq!(a.view, config::MINE);
        assert!(a.status_filter.is_empty());
        assert_eq!(a.visible.len(), 1);
    }
    #[test]
    fn filter_lists_preserve_custom_values_and_status_dropdown_preselects() {
        let mut a = app();
        a.assignee_filter = "other@example.com".into();
        a.status_filter = "Blocked".into();
        a.filters_dialog();
        let Some(Dialog::Filters {
            choices, selected, ..
        }) = &a.dialog
        else {
            panic!()
        };
        assert_eq!(choices[0][selected[0]].value, "other@example.com");
        assert_eq!(choices[1][selected[1]].value, "Blocked");
        a.status_picker(true);
        let Some(Dialog::Filters {
            choices,
            selected,
            checked,
            active,
            ..
        }) = &a.dialog
        else {
            panic!()
        };
        assert_eq!(*active, 1);
        assert_eq!(choices[1][selected[1]].value, "Blocked");
        assert!(checked[1].contains("Blocked"));
        a.assignee_picker(true);
        if let Some(Dialog::Filters {
            choices, selected, ..
        }) = &mut a.dialog
        {
            selected[0] = choices[0].len() - 1;
        }
        a.event(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));
        let Some(Dialog::Filters {
            custom: Some(input),
            checked,
            ..
        }) = &a.dialog
        else {
            panic!()
        };
        assert_eq!(input.column, 0);
        assert!(input.value.is_empty());
        assert!(checked[0].contains("other@example.com"));
        assert!(checked[1].contains("Blocked"));
    }
    #[test]
    fn open_filter_lists_update_after_sync_without_losing_staged_selections() {
        let mut a = app();
        a.assignee_picker(true);
        a.event(Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)));
        let mut workspace = a.workspace.clone().unwrap();
        let mut new = data::demo_tickets().remove(2);
        new.key = "DEV-104".into();
        new.fields["assignee"] = serde_json::json!({"displayName":"Taylor","accountId":"taylor"});
        new.fields["status"] = serde_json::json!({"name":"Blocked"});
        workspace.items.push(new);
        a.message(Message::Synced {
            generation: a.generation,
            result: Ok((workspace, vec![])),
        });
        let Some(Dialog::Filters {
            choices, selected, ..
        }) = &a.dialog
        else {
            panic!()
        };
        assert_eq!(choices[0][selected[0]].value, "@me");
        assert!(choices[0].iter().any(|c| c.value == "taylor"));
        assert!(choices[1].iter().any(|c| c.value == "Blocked"));
        assert!(a.assignee_filter.is_empty());
    }
    #[test]
    fn cached_numbers_show_the_ticket_during_the_input_event_without_a_timer() {
        let mut a = app();
        for c in "102".chars() {
            assert!(a.event(key(c, KeyModifiers::NONE)).is_empty());
        }
        assert_eq!(a.detail_key, "DEV-102");
        assert_eq!(a.selected_ticket().unwrap().key, "DEV-102");
        assert!(a.search_due.is_none());
        assert!(!a.loading);
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
    fn text_search_changes_selection_immediately_and_does_not_clone_the_dataset() {
        let mut a = app();
        let address = a.items.as_ptr();
        a.event(key('/', KeyModifiers::NONE));
        for c in "confirmation".chars() {
            assert!(a.event(key(c, KeyModifiers::NONE)).is_empty());
            assert!(a.search_due.is_none());
            assert_eq!(a.items.as_ptr(), address);
        }
        assert_eq!(a.visible.len(), 1);
        assert_eq!(a.detail_key, "DEV-103");
        assert_eq!(a.selected_ticket().unwrap().key, "DEV-103");
        a.event(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));
        assert_eq!(a.items.as_ptr(), address);
    }
    #[test]
    fn deleting_all_lookup_digits_restores_the_cached_backlog() {
        let mut a = app();
        for c in "102".chars() {
            a.event(key(c, KeyModifiers::NONE));
        }
        for _ in 0..3 {
            a.event(Event::Key(KeyEvent::new(
                KeyCode::Backspace,
                KeyModifiers::NONE,
            )));
        }
        assert!(matches!(a.search_mode, SearchMode::None));
        assert!(!a.lookup_active);
        assert_eq!(a.items.len(), 3);
        assert_eq!(a.visible.len(), 3);
        assert!(a.search_due.is_none());
    }
    #[test]
    fn filtered_empty_backlog_refresh_keeps_membership_and_f6_restores_rows_locally() {
        let mut a = app();
        let mut w = a.workspace.clone().unwrap();
        w.board = Some(["DEV-101".into(), "DEV-103".into()].into());
        w.reindex();
        a.message(Message::Synced {
            generation: a.generation,
            result: Ok((w.clone(), vec![])),
        });
        a.switch_view(config::BOARD);
        a.status_filter = "In Progress".into();
        a.reload();
        assert_eq!(a.visible.len(), 1);
        a.switch_view(config::BACKLOG);
        assert_eq!(a.visible.len(), 1);
        assert!(a.status_filter.is_empty());
        a.status_filter = "In Progress".into();
        a.reload();
        assert!(a.visible.is_empty());
        assert_eq!(a.workspace.as_ref().unwrap().view_count(config::BACKLOG), 1);
        assert!(a.status.contains("1 in Backlog"));
        assert!(a.status.contains("filters active"));
        a.refresh_all();
        a.message(Message::Synced {
            generation: a.generation,
            result: Ok((w, vec![])),
        });
        assert!(a.visible.is_empty());
        assert_eq!(a.status_filter, "In Progress");
        assert!(
            a.event(Event::Key(KeyEvent::new(KeyCode::F(6), KeyModifiers::NONE)))
                .is_empty()
        );
        assert_eq!(a.view, config::BACKLOG);
        assert_eq!(a.selected_ticket().unwrap().key, "DEV-102");
        assert!(a.status_filter.is_empty());
        a.dialog = Some(Dialog::Editor {
            title: "Synthetic comment".into(),
            kind: "comment".into(),
            key: "DEV-102".into(),
            value: "keep draft".into(),
            cursor: 10,
        });
        a.event(Event::Key(KeyEvent::new(KeyCode::F(6), KeyModifiers::NONE)));
        let Some(Dialog::Editor { value, .. }) = a.dialog else {
            panic!()
        };
        assert_eq!(value, "keep draft");
    }
    #[test]
    fn epic_containers_cannot_be_moved_at_entry_or_confirmation() {
        let mut a = app();
        a.read_only = false;
        a.settings.board_id = Some(17);
        let mut w = a.workspace.clone().unwrap();
        w.board = Some(["DEV-101".into()].into());
        w.items[0].fields["issuetype"] = serde_json::json!({"name":"Epic","hierarchyLevel":1});
        w.reindex();
        a.message(Message::Synced {
            generation: a.generation,
            result: Ok((w, vec![])),
        });
        a.view = config::BOARD;
        a.apply_local();
        a.action_dialog('m');
        assert!(a.dialog.is_none());
        assert!(a.status.contains("Epic containers"));
        a.confirm(Action::new("backlog", "DEV-101", "17"));
        assert!(a.event(key('y', KeyModifiers::NONE)).is_empty());
        assert!(!a.writing);
    }
    #[test]
    fn cached_inventory_keeps_epics_and_rank_order_but_excludes_subtasks() {
        let dir = tempfile::tempdir().unwrap();
        let mut a = app();
        a.cache = Cache {
            directory: Some(dir.path().into()),
        };
        let mut workspace = a.workspace.clone().unwrap();
        workspace.items.reverse();
        let mut epic = data::demo_tickets().remove(0);
        epic.key = "DEV-1".into();
        epic.fields["issuetype"] = serde_json::json!({"name":"Epic"});
        let mut child = epic.clone();
        child.key = "DEV-2".into();
        child.fields["issuetype"] = serde_json::json!({"name":"Sub-Task","subtask":true});
        workspace.items.splice(0..0, [epic, child]);
        a.cache.put(&a.workspace_key(), &workspace).unwrap();
        let mut next = App::new(
            a.backend.clone(),
            a.cache.clone(),
            a.settings.clone(),
            true,
            500,
            PathBuf::new(),
        );
        next.reload();
        assert_eq!(
            next.items
                .iter()
                .map(|t| t.key.as_str())
                .collect::<Vec<_>>(),
            ["DEV-1", "DEV-103", "DEV-102", "DEV-101"]
        );
    }
    #[test]
    fn switching_views_returns_to_first_ranked_row_instead_of_old_scroll_position() {
        let mut a = app();
        a.selected = 2;
        *a.table.offset_mut() = 2;
        a.select_current();
        a.switch_view(config::AVAILABLE);
        a.switch_view(config::BACKLOG);
        assert_eq!(a.selected, 0);
        assert_eq!(a.table.offset(), 0);
        assert_eq!(a.detail_key, "DEV-101");
    }
    #[test]
    fn membership_tabs_and_available_use_both_sections_without_sorting() {
        let mut a = app();
        a.settings.board_id = Some(17);
        let w = a.workspace.as_mut().unwrap();
        w.board = Some(HashSet::from(["DEV-101".into(), "DEV-103".into()]));
        w.items[0].fields["status"] =
            serde_json::json!({"name":"Done","statusCategory":{"key":"done"}});
        w.items[0].fields["assignee"] = serde_json::Value::Null;
        w.reindex();
        a.items_are_workspace = false;
        a.last_query = a.settings.query(config::BACKLOG, "", "", "").unwrap();
        a.apply_local();
        for (view, keys) in [
            (config::BOARD, vec!["DEV-101", "DEV-103"]),
            (config::BACKLOG, vec!["DEV-102"]),
            (config::AVAILABLE, vec!["DEV-101", "DEV-102"]),
        ] {
            assert!(a.switch_view(view).is_empty());
            assert_eq!(
                a.visible
                    .iter()
                    .map(|i| a.items[*i].key.as_str())
                    .collect::<Vec<_>>(),
                keys
            );
        }
        a.switch_view(config::BOARD);
        a.status_filter = "Done".into();
        a.reload();
        assert_eq!(a.visible.len(), 1);
        assert_eq!(a.selected_ticket().unwrap().key, "DEV-101");
    }
    #[test]
    fn moving_requires_verified_membership_and_confirmation_at_both_read_only_boundaries() {
        let mut a = app();
        a.settings.board_id = Some(17);
        a.workspace.as_mut().unwrap().board = Some(HashSet::from(["DEV-101".into()]));
        a.action_dialog('m');
        let Some(Dialog::Confirm(ref action)) = a.dialog else {
            panic!("confirmation missing")
        };
        assert_eq!(
            (
                action.kind.as_str(),
                action.key.as_str(),
                action.value.as_str()
            ),
            ("backlog", "DEV-101", "17")
        );
        // DEV-101 is the selected legacy Backlog row here; membership, not tab,
        // determines destination even after a stale/filtered selection.
        assert_eq!(action.kind, "backlog");
        a.read_only = true;
        assert!(a.event(key('y', KeyModifiers::NONE)).is_empty());
        assert!(!a.writing);
        a.action_dialog('m');
        assert!(a.dialog.is_none());
        a.read_only = false;
        a.action_dialog('m');
        let jobs = a.event(key('y', KeyModifiers::NONE));
        assert!(
            matches!(&jobs[..],[Job::Write(Action{kind,key,..})] if kind=="backlog"&&key=="DEV-101")
        );
        let mut a = app();
        a.settings.board_id = Some(17);
        a.action_dialog('m');
        assert!(a.dialog.is_none());
    }
    #[test]
    fn membership_cache_is_board_specific_and_preserves_order_on_restart() {
        let dir = tempfile::tempdir().unwrap();
        let mut a = app();
        a.settings.board_id = Some(17);
        a.last_query = a.settings.query(config::BACKLOG, "", "", "").unwrap();
        a.cache = Cache {
            directory: Some(dir.path().into()),
        };
        a.workspace.as_mut().unwrap().board = Some(HashSet::from(["DEV-101".into()]));
        a.cache
            .put(&a.workspace_key(), a.workspace.as_ref().unwrap())
            .unwrap();
        let mut next = App::new(
            a.backend.clone(),
            a.cache.clone(),
            a.settings.clone(),
            true,
            500,
            PathBuf::new(),
        );
        next.reload();
        next.switch_view(config::BOARD);
        assert_eq!(next.selected_ticket().unwrap().key, "DEV-101");
        let old_key = next.workspace_key();
        next.settings.board_id = Some(18);
        assert_ne!(next.workspace_key(), old_key);
        assert!(next.cache.get::<Workspace>(&next.workspace_key()).is_none());
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
