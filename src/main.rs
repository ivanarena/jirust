use anyhow::{Result, bail};
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum, parser::ValueSource};
use crossterm::{
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use jirust::{
    app::{App, Job, Message},
    backend::{Backend, Kind},
    config::{Cache, Settings},
    data,
    environment::Environment,
    movement::{self, Destination},
    render,
};
use ratatui::{Terminal, backend::CrosstermBackend, layout::Size};
use ratatui_image::{
    Resize,
    picker::{Picker, ProtocolType},
};
use std::{
    io::{self, BufRead, IsTerminal, Read, Write},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{process::Command, sync::mpsc};

#[derive(Parser)]
#[command(
    version,
    about = "A simple two-pane Jira TUI backed by your existing CLI"
)]
struct Options {
    #[arg(long, default_value = "auto", global = true)]
    backend: String,
    /// Private dotenv file (default: JIRUST_ENV_FILE, ./.env, then user config).
    #[arg(long, global = true)]
    env_file: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<CliCommand>,
    #[arg(long, global = true)]
    demo: bool,
    #[arg(long)]
    cli_bin: Option<PathBuf>,
    #[arg(long, global = true)]
    read_only: bool,
    /// Opt in to Jira Software APIs with a local token or existing acli credentials.
    #[arg(long, global = true,value_parser=clap::value_parser!(u64).range(1..))]
    board: Option<u64>,
    #[arg(long)]
    no_cache: bool,
    #[arg(long)]
    project: Option<String>,
    #[arg(long)]
    prefix: Option<String>,
    #[arg(long)]
    jql: Option<String>,
    /// Optional display label for this scope (does not change membership/JQL).
    #[arg(long)]
    scope_name: Option<String>,
    /// Initial view; otherwise restore the saved view.
    #[arg(long, value_enum)]
    view: Option<StartView>,
    /// Browser/clipboard-only Jira base URL, optionally with a context path.
    /// Never used for authenticated requests. Default: pinned site/ticket metadata.
    #[arg(long)]
    browse_url: Option<String>,
    #[arg(long,default_value_t=60,value_parser=clap::value_parser!(u64).range(15..=86400))]
    refresh_seconds: u64,
    #[arg(long,default_value_t=120,value_parser=clap::value_parser!(u64).range(15..=86400))]
    refresh_retry_seconds: u64,
    #[arg(long,default_value_t=500,value_parser=clap::value_parser!(u16).range(1..=2000))]
    limit: u16,
    #[arg(long)]
    check: bool,
    #[arg(long)]
    attachment_dir: Option<PathBuf>,
    #[arg(long,default_value="auto",value_parser=["auto","kitty","halfblocks"])]
    image_protocol: String,
    #[arg(
        long = "no-inline-images",
        alias = "no-images",
        conflicts_with = "images"
    )]
    no_images: bool,
    /// Enable automatic images even when disabled in the environment.
    #[arg(long)]
    images: bool,
    /// Select rows normally without writing to the clipboard.
    #[arg(long, conflicts_with = "copy_on_click")]
    no_copy_on_click: bool,
    /// Enable row-click copies even when disabled in the environment.
    #[arg(long)]
    copy_on_click: bool,
}
#[derive(Clone, Copy, clap::ValueEnum)]
enum StartView {
    Mine,
    Available,
    Board,
    Backlog,
}
impl StartView {
    fn index(self) -> usize {
        match self {
            Self::Mine => jirust::config::MINE,
            Self::Available => jirust::config::AVAILABLE,
            Self::Board => jirust::config::BOARD,
            Self::Backlog => jirust::config::BACKLOG,
        }
    }
}
#[derive(Subcommand)]
enum CliCommand {
    /// Move exactly one ticket between verified Board and Backlog membership.
    Move {
        key: String,
        #[arg(long, value_enum)]
        to: Destination,
        /// Explicitly confirm without an interactive prompt.
        #[arg(long)]
        yes: bool,
    },
    /// Authenticate acli using the local URL, email and API token via stdin.
    Auth,
}
impl Options {
    fn apply_environment(&mut self, env: &Environment, matches: &clap::ArgMatches) -> Result<()> {
        let supplied = |name: &str| matches.value_source(name) == Some(ValueSource::CommandLine);
        if self.demo || self.backend == "demo" {
            return Ok(());
        }
        if !supplied("backend")
            && let Some(v) = env.value("JIRA_BACKEND")
        {
            self.backend = v;
        }
        if !["auto", "acli", "jira", "demo"].contains(&self.backend.as_str()) {
            bail!("Invalid JIRA_BACKEND");
        }
        if self.backend == "demo" {
            return Ok(());
        }
        // A new explicit project must not inherit another project's env board,
        // prefix or JQL. Explicit flags remain the highest precedence.
        let project_changed = supplied("project")
            && !env.value("JIRA_PROJECT").is_some_and(|p| {
                self.project
                    .as_ref()
                    .is_some_and(|v| v.eq_ignore_ascii_case(&p))
            });
        for (name, key, field) in [
            ("project", "JIRA_PROJECT", &mut self.project),
            ("prefix", "JIRA_PREFIX", &mut self.prefix),
            ("jql", "JIRA_JQL", &mut self.jql),
            ("scope_name", "JIRA_SCOPE_NAME", &mut self.scope_name),
        ] {
            if !(supplied(name) || (project_changed && name != "project"))
                && let Some(v) = env.value(key)
            {
                *field = Some(v);
            }
        }
        if !supplied("browse_url") {
            self.browse_url = env.value("JIRA_BROWSE_URL");
        }
        if !supplied("view")
            && let Some(v) = env.value("JIRA_VIEW")
        {
            self.view = Some(StartView::from_str(&v, true).map_err(|_| {
                anyhow::anyhow!("Invalid JIRA_VIEW; use mine, available, board or backlog")
            })?);
        }
        for (flag, key, seconds) in [
            (
                "refresh_seconds",
                "JIRA_REFRESH_SECONDS",
                &mut self.refresh_seconds,
            ),
            (
                "refresh_retry_seconds",
                "JIRA_REFRESH_RETRY_SECONDS",
                &mut self.refresh_retry_seconds,
            ),
        ] {
            if !supplied(flag)
                && let Some(v) = env.number(key, 86400)?
            {
                anyhow::ensure!(v >= 15, "Invalid {key}; use 15..86400 seconds");
                *seconds = v;
            }
        }
        if !supplied("no_images")
            && !supplied("images")
            && let Some(v) = env.boolean("JIRA_NO_IMAGES")?
        {
            self.no_images = v;
        }
        if !supplied("no_copy_on_click")
            && !supplied("copy_on_click")
            && let Some(v) = env.boolean("JIRA_NO_COPY_ON_CLICK")?
        {
            self.no_copy_on_click = v;
        }
        if !supplied("cli_bin")
            && let Some(v) = env.value("JIRA_CLI_BIN")
        {
            self.cli_bin = Some(v.into());
        }
        if !supplied("board")
            && !project_changed
            && let Some(v) = env.number("JIRA_BOARD_ID", u64::MAX)?
        {
            self.board = Some(v);
        }
        if !supplied("limit")
            && let Some(v) = env.number("JIRA_LIMIT", 2000)?
        {
            self.limit = v as u16;
        }
        if !supplied("read_only")
            && let Some(v) = env.boolean("JIRA_READ_ONLY")?
        {
            self.read_only = v;
        }
        if !supplied("no_cache")
            && let Some(v) = env.boolean("JIRA_NO_CACHE")?
        {
            self.no_cache = v;
        }
        if !supplied("attachment_dir")
            && let Some(v) = env.value("JIRA_ATTACHMENT_DIR")
        {
            self.attachment_dir = Some(v.into());
        }
        if !supplied("image_protocol")
            && let Some(v) = env.value("JIRA_IMAGE_PROTOCOL")
        {
            self.image_protocol = v;
        }
        if !["auto", "kitty", "halfblocks"].contains(&self.image_protocol.as_str()) {
            bail!("Invalid JIRA_IMAGE_PROTOCOL");
        }
        Ok(())
    }
    fn validate(&self) -> Result<()> {
        if let Some(p) = &self.prefix {
            anyhow::ensure!(
                p.is_empty()
                    || (p.len() <= 64
                        && p.as_bytes()[0].is_ascii_alphabetic()
                        && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')),
                "Invalid ticket prefix (value redacted); use a Jira project key"
            );
        }
        if let Some(label) = &self.scope_name {
            anyhow::ensure!(
                label.len() <= 128 && !label.chars().any(char::is_control),
                "Invalid scope name (value redacted); use at most 128 bytes without control characters"
            );
        }
        if let Some(base) = &self.browse_url {
            jirust::links::BrowseBase::parse(base)?;
        }
        Ok(())
    }
    fn apply_scope(&self, settings: &mut Settings) -> bool {
        let changed = self.project.is_some()
            || self.prefix.is_some()
            || self.jql.is_some()
            || self.board.is_some()
            || self.scope_name.is_some()
            || self.view.is_some();
        if let Some(project) = &self.project {
            let project = project.to_uppercase();
            if settings.project != project {
                settings.board_id = None;
            }
            settings.project = project;
            if self.prefix.is_none() {
                settings.prefix.clear();
            }
            if self.jql.is_none() {
                settings.scope_jql.clear();
            }
            settings.scope_name.clear();
        }
        if let Some(prefix) = &self.prefix {
            settings.prefix = prefix.to_uppercase();
        }
        if let Some(jql) = &self.jql {
            settings.scope_jql = jql.clone();
            settings.scope_name.clear();
        }
        if let Some(id) = self.board {
            settings.board_id = Some(id);
        }
        if let Some(label) = &self.scope_name {
            settings.scope_name = label.clone();
        }
        if let Some(view) = self.view {
            settings.last_view = view.index();
        }
        changed
    }
}
struct TerminalGuard;
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(
            io::stdout(),
            PopKeyboardEnhancementFlags,
            DisableBracketedPaste,
            DisableMouseCapture,
            LeaveAlternateScreen
        );
        let _ = disable_raw_mode();
    }
}
#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<()> {
    let matches = Options::command().get_matches();
    let mut opts = Options::from_arg_matches(&matches)?;
    let env = if opts.demo || opts.backend == "demo" {
        Environment::default()
    } else {
        if matches!(opts.command, Some(CliCommand::Auth)) {
            Environment::load_for_auth(opts.env_file.as_deref())?
        } else {
            Environment::load(opts.env_file.as_deref())?
        }
    };
    opts.apply_environment(&env, &matches)?;
    opts.validate()?;
    if opts.check && opts.command.is_some() {
        bail!("--check cannot be combined with a command; no operation performed");
    }
    let backend = Backend::new(
        if opts.demo { "demo" } else { &opts.backend },
        opts.cli_bin.clone(),
    )?;
    if matches!(opts.command, Some(CliCommand::Auth)) {
        anyhow::ensure!(
            backend.kind != Kind::Demo,
            "Demo does not support authentication"
        );
        let credentials = jirust::auth_setup::prepare(env, opts.env_file.as_deref())?;
        backend.authenticate(&credentials).await?;
        println!("acli authenticated from local environment; token not displayed");
        return Ok(());
    }
    let credentials = if backend.kind == Kind::Demo {
        None
    } else {
        env.credentials()?
    };
    backend.configure_environment(credentials)?;
    backend.configure_links(
        opts.browse_url
            .as_deref()
            .map(jirust::links::BrowseBase::parse)
            .transpose()?,
    );
    let mut settings = if backend.kind == Kind::Demo {
        Settings {
            project: "DEV".into(),
            last_view: jirust::config::MINE,
            board_id: Some(1),
            ..Default::default()
        }
    } else {
        Settings::load()?
    };
    let scope_override = opts.apply_scope(&mut settings);
    backend.configure_board(settings.board_id)?;
    let limit = if backend.kind == Kind::Jira {
        usize::from(opts.limit).min(100)
    } else {
        usize::from(opts.limit)
    };
    if let Some(CliCommand::Move { key, to, yes }) = opts.command {
        let Some(action) = movement::prepare(&backend, &settings, &key, to, opts.read_only).await?
        else {
            println!("{key} is already in {}; nothing changed", to.name());
            return Ok(());
        };
        println!(
            "Move {key}: {} → {} (Board {}). No workflow transition requested; Jira sets destination Rank.",
            to.source(),
            to.name(),
            action.value
        );
        let confirmed = if yes {
            true
        } else {
            if !io::stdin().is_terminal() {
                bail!(
                    "Interactive confirmation required; use --yes only to explicitly confirm this single-ticket move"
                );
            }
            print!("Type {key} to confirm (anything else cancels): ");
            io::stdout().flush()?;
            let mut input = String::new();
            io::stdin().lock().take(128).read_line(&mut input)?;
            input.trim() == key
        };
        if !confirmed {
            println!("Cancelled; no ticket changed");
            return Ok(());
        }
        movement::execute(&backend, &settings, &action, to, opts.read_only, confirmed).await?;
        println!("{key} is now in {}; refresh the TUI with F5", to.name());
        return Ok(());
    }
    if opts.check {
        let q = settings.query(jirust::config::BACKLOG, "", "", "")?;
        let mine = settings.query(jirust::config::MINE, "", "", "")?;
        let (workspace, _) = backend.workspace(&q, &mine, None).await?;
        let counts = [
            jirust::config::MINE,
            jirust::config::AVAILABLE,
            jirust::config::BOARD,
            jirust::config::BACKLOG,
        ]
        .map(|view| workspace.view_count(view));
        let epics = workspace.items.iter().filter(|t| t.is_epic()).count();
        let estimated = workspace
            .items
            .iter()
            .filter(|t| t.story_points().is_some())
            .count();
        let items = workspace.items;
        if let Some(t) = items.first() {
            backend.get(&t.key).await?;
        };
        println!(
            "backend={} search=ok tickets={} detail={} mine={} available={} board={} backlog={} epics={} story_points={} (read-only; no records changed)",
            backend.kind.name(),
            items.len(),
            if items.is_empty() { "skipped" } else { "ok" },
            counts[0],
            counts[1],
            counts[2],
            counts[3],
            epics,
            estimated
        );
        return Ok(());
    }
    if !io::stdin().is_terminal() {
        bail!("Run jirust in a terminal, or use --check")
    }
    if scope_override && backend.kind != Kind::Demo {
        settings.save()?;
    }
    let scope = backend.scope().await;
    let cache = Cache::new(&scope, opts.no_cache || backend.kind == Kind::Demo)?;
    let attachment_dir = attachment_root(backend.kind, &scope, opts.attachment_dir)?;
    enable_raw_mode()?;
    let guard = TerminalGuard;
    execute!(
        io::stdout(),
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )?;
    // Query before reading keyboard events, never while the event loop is running.
    let mut picker = if opts.image_protocol == "auto" {
        Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks())
    } else {
        Picker::halfblocks()
    };
    if opts.image_protocol == "kitty" {
        picker.set_protocol_type(ProtocolType::Kitty)
    }
    execute!(
        io::stdout(),
        PushKeyboardEnhancementFlags(
            KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
        )
    )?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let (tx, mut rx) = mpsc::unbounded_channel();
    let (clipboard_tx, clipboard_rx) = tokio::sync::watch::channel(None);
    let clipboard_messages = tx.clone();
    tokio::spawn(jirust::clipboard::run(clipboard_rx, move |completion| {
        let _ = clipboard_messages.send(Message::Copied(completion));
    }));
    let mut app = App::new(
        backend.clone(),
        cache,
        settings,
        opts.read_only,
        limit,
        attachment_dir,
    );
    app.automatic_images = !opts.no_images;
    app.copy_on_click = !opts.no_copy_on_click;
    app.refresh_interval = Duration::from_secs(opts.refresh_seconds);
    app.refresh_retry = Duration::from_secs(opts.refresh_retry_seconds);
    let jobs = app.reload();
    dispatch(jobs, &backend, &picker, &tx, &clipboard_tx);
    let mut dirty = true;
    let mut last_busy = Instant::now();
    while !app.quit {
        // Drain queued typing before background work, and paint input bursts
        // once instead of rebuilding the ticket document for every byte.
        for _ in 0..32 {
            if app.quit || !event::poll(Duration::ZERO)? {
                break;
            }
            let jobs = app.event(event::read()?);
            dispatch(jobs, &backend, &picker, &tx, &clipboard_tx);
            dirty = true;
        }
        if app.quit {
            break;
        }
        for _ in 0..8 {
            if event::poll(Duration::ZERO)? {
                break;
            }
            let Ok(mut message) = rx.try_recv() else {
                break;
            };
            if let Message::Copied(completion) = &mut message
                && completion.request.generation == app.copy_generation
                && completion
                    .result
                    .as_ref()
                    .is_ok_and(|method| *method == jirust::clipboard::Method::Terminal)
            {
                completion.result = jirust::clipboard::terminal(&completion.request.url)
                    .map(|_| jirust::clipboard::Method::Terminal);
            }
            let jobs = app.message(message);
            dispatch(jobs, &backend, &picker, &tx, &clipboard_tx);
            dirty = true;
        }
        let jobs = app.advance();
        if !jobs.is_empty() {
            dispatch(jobs, &backend, &picker, &tx, &clipboard_tx);
            dirty = true
        }
        if app.dirty {
            dirty = true;
            app.dirty = false;
        }
        if (app.loading || app.syncing || app.writing || app.awaiting_image)
            && last_busy.elapsed() > Duration::from_millis(500)
        {
            dirty = true;
            last_busy = Instant::now()
        }
        if dirty {
            terminal.draw(|f| render::draw(f, &mut app))?;
            dirty = false
        }
        if event::poll(Duration::from_millis(40))? {
            let jobs = app.event(event::read()?);
            dispatch(jobs, &backend, &picker, &tx, &clipboard_tx);
            dirty = true
        }
    }
    drop(terminal);
    drop(guard);
    let cache_writer = app.cache_writer.clone();
    let _ = tokio::task::spawn_blocking(move || cache_writer.flush()).await;
    Ok(())
}
fn attachment_root(kind: Kind, scope: &str, explicit: Option<PathBuf>) -> Result<PathBuf> {
    match explicit {
        Some(path) => Ok(path),
        // Demo must not migrate/chmod the user's actual cache directories.
        None if kind == Kind::Demo => Ok(std::env::temp_dir().join("jirust-demo-attachments")),
        None => jirust::config::attachment_directory(scope),
    }
}
fn dispatch(
    jobs: Vec<Job>,
    backend: &Arc<Backend>,
    picker: &Picker,
    tx: &mpsc::UnboundedSender<Message>,
    clipboard: &tokio::sync::watch::Sender<Option<jirust::clipboard::Request>>,
) {
    for job in jobs {
        if let Job::CopyLink(request) = &job {
            clipboard.send_replace(Some(request.clone()));
            continue;
        }
        let backend = backend.clone();
        let tx = tx.clone();
        let picker = picker.clone();
        tokio::spawn(async move {
            let message = match job {
                Job::CopyLink(_) => return,
                Job::Projects { generation } => Message::Projects {
                    generation,
                    result: backend.projects().await,
                },
                Job::Sync {
                    generation,
                    base,
                    mine,
                    changed,
                } => Message::Synced {
                    generation,
                    result: backend
                        .workspace_progress(&base, &mine, changed.as_deref(), |stage| {
                            let _ = tx.send(Message::SyncProgress { generation, stage });
                        })
                        .await,
                },

                Job::Detail {
                    key,
                    lookup,
                    revision,
                    background,
                    cancel,
                } => Message::Detail {
                    key: key.clone(),
                    lookup,
                    revision,
                    background,
                    result: detail_read(&backend, &key, cancel).await,
                },
                Job::Comments(key) => Message::Comments {
                    key: key.clone(),
                    result: backend.comments(&key).await,
                },
                Job::Write(action) => Message::Written {
                    action: action.clone(),
                    result: backend.apply(&action).await,
                },
                Job::Move {
                    action,
                    settings,
                    read_only,
                } => {
                    let result =
                        jirust::movement::confirmed(&backend, &settings, &action, read_only).await;
                    Message::Written { action, result }
                }
                Job::Browser(url) => {
                    let result = async {
                        if !data::valid_url(&url) {
                            bail!("Only ordinary HTTP(S) links may open")
                        };
                        let program = if cfg!(target_os = "macos") {
                            "open"
                        } else {
                            "xdg-open"
                        };
                        let status = Command::new(program)
                            .arg(url)
                            .stdin(std::process::Stdio::null())
                            .stdout(std::process::Stdio::null())
                            .kill_on_drop(true)
                            .status()
                            .await?;
                        if !status.success() {
                            bail!("Browser opener failed")
                        };
                        Ok("Opened in browser".into())
                    }
                    .await;
                    Message::Notice(result)
                }
                Job::AttachmentImage {
                    generation,
                    key,
                    attachment,
                    cache,
                    size,
                    inline,
                    cancel,
                } => {
                    let id = attachment.id.clone();
                    let name = attachment.name.clone();
                    let result = attachment_protocol(
                        &backend,
                        &key,
                        &attachment,
                        cache,
                        picker,
                        if inline {
                            ImageLayout::Inline(size.0)
                        } else {
                            ImageLayout::Preview(size)
                        },
                        cancel,
                    )
                    .await;
                    if inline {
                        Message::InlineImage {
                            generation,
                            key,
                            id,
                            result: result.and_then(|p| match p {
                                PreparedImage::Inline(p) => Ok(p),
                                _ => Err(anyhow::anyhow!("Unexpected image mode")),
                            }),
                        }
                    } else {
                        Message::Image {
                            generation,
                            name,
                            result: result.and_then(|p| match p {
                                PreparedImage::Preview(p) => Ok(p),
                                _ => Err(anyhow::anyhow!("Unexpected image mode")),
                            }),
                        }
                    }
                }
                Job::Preview {
                    generation,
                    path,
                    size,
                } => {
                    let name = path
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "image".into());
                    let permit = IMAGE_DECODERS.clone().acquire_owned().await;
                    let result = tokio::task::spawn_blocking(move || {
                        let _permit = permit?;
                        let image = jirust::images::decode(&jirust::images::local_bytes(&path)?)?;
                        Ok(picker.new_protocol(
                            image,
                            Size::new(size.0, size.1),
                            Resize::Fit(None),
                        )?)
                    })
                    .await
                    .unwrap_or_else(|e| Err(e.into()));
                    Message::Image {
                        generation,
                        name,
                        result,
                    }
                }
            };
            let _ = tx.send(message);
        });
    }
}
async fn detail_read(
    backend: &Backend,
    key: &str,
    cancel: Option<tokio::sync::watch::Receiver<bool>>,
) -> Result<data::Ticket> {
    if let Some(mut cancel) = cancel {
        anyhow::ensure!(!*cancel.borrow(), "Background read cancelled");
        tokio::select! { result=backend.get(key)=>result, _=cancel.changed()=>Err(anyhow::anyhow!("Background read cancelled")) }
    } else {
        backend.get(key).await
    }
}
static IMAGE_DECODERS: std::sync::LazyLock<Arc<tokio::sync::Semaphore>> =
    std::sync::LazyLock::new(|| Arc::new(tokio::sync::Semaphore::new(2)));
enum ImageLayout {
    Inline(u16),
    Preview((u16, u16)),
}
enum PreparedImage {
    Inline(jirust::images::InlineImage),
    Preview(ratatui_image::protocol::Protocol),
}
async fn attachment_protocol(
    backend: &Backend,
    key: &str,
    attachment: &data::Attachment,
    cache: Option<PathBuf>,
    picker: Picker,
    layout: ImageLayout,
    mut cancel: tokio::sync::watch::Receiver<bool>,
) -> Result<PreparedImage> {
    anyhow::ensure!(!*cancel.borrow(), "Image cancelled");
    let decoder_cancel = cancel.clone();
    let operation = async {
        let cached = cache.as_deref().and_then(jirust::images::cached_bytes);
        let bytes = backend.image_bytes(key, attachment, cached).await?;
        let permit = IMAGE_DECODERS.clone().acquire_owned().await?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let image = match jirust::images::decode(&bytes) {
                Ok(i) => i,
                Err(e) => {
                    if let Some(p) = &cache {
                        let _ = std::fs::remove_file(p);
                    }
                    return Err(e);
                }
            };
            anyhow::ensure!(!*decoder_cancel.borrow(), "Image cancelled");
            if let Some(p) = &cache {
                let _ = jirust::images::store(p, &bytes);
            }
            match layout {
                ImageLayout::Inline(width) => Ok(PreparedImage::Inline(
                    jirust::images::make_inline(&picker, image, width)?,
                )),
                ImageLayout::Preview(size) => Ok(PreparedImage::Preview(picker.new_protocol(
                    image,
                    Size::new(size.0, size.1),
                    Resize::Fit(None),
                )?)),
            }
        })
        .await
        .map_err(|_| anyhow::anyhow!("Image decoder failed"))?
    };
    tokio::select! {result=operation=>result,_=cancel.changed()=>Err(anyhow::anyhow!("Image cancelled"))}
}
#[cfg(test)]
mod tests {
    use super::*;
    fn options(args: &[&str]) -> (Options, clap::ArgMatches) {
        let matches = Options::command().try_get_matches_from(args).unwrap();
        (Options::from_arg_matches(&matches).unwrap(), matches)
    }
    fn env(text: &str) -> Environment {
        let d = tempfile::tempdir().unwrap();
        let mut f = tempfile::NamedTempFile::new_in(d.path()).unwrap();
        f.write_all(text.as_bytes()).unwrap();
        Environment::load(Some(f.path())).unwrap()
    }
    #[tokio::test]
    async fn cancelled_image_jobs_do_not_start_network_or_cli_reads() {
        let backend = Backend::new("demo", None).unwrap();
        let (_, rx) = tokio::sync::watch::channel(true);
        let attachment = data::Attachment {
            id: "23".into(),
            mime: "image/png".into(),
            ..Default::default()
        };
        let e = attachment_protocol(
            &backend,
            "DEV-101",
            &attachment,
            None,
            Picker::halfblocks(),
            ImageLayout::Inline(40),
            rx,
        )
        .await
        .err()
        .unwrap();
        assert!(e.to_string().contains("cancelled"));
    }
    #[test]
    fn demo_attachment_defaults_do_not_resolve_personal_config_or_cache() {
        assert_eq!(
            attachment_root(Kind::Demo, "demo", None).unwrap(),
            std::env::temp_dir().join("jirust-demo-attachments")
        );
        let d = tempfile::tempdir().unwrap();
        assert_eq!(
            attachment_root(Kind::Demo, "demo", Some(d.path().into())).unwrap(),
            d.path()
        );
    }
    #[test]
    fn generalized_environment_settings_obey_flags_and_saved_scope_defaults() {
        let e = env(
            "JIRA_PROJECT=DEV\nJIRA_PREFIX=ALT\nJIRA_SCOPE_NAME='Demo scope'\nJIRA_VIEW=backlog\nJIRA_BROWSE_URL=https://jira.example.org/jira\nJIRA_REFRESH_SECONDS=90\nJIRA_REFRESH_RETRY_SECONDS=180\nJIRA_NO_IMAGES=true\nJIRA_NO_COPY_ON_CLICK=true\n",
        );
        let (mut o, m) = options(&["jirust"]);
        o.apply_environment(&e, &m).unwrap();
        o.validate().unwrap();
        let mut s = Settings::default();
        o.apply_scope(&mut s);
        assert_eq!(s.prefix(), "ALT");
        assert_eq!(s.scope_name, "Demo scope");
        assert_eq!(s.last_view, jirust::config::BACKLOG);
        assert_eq!((o.refresh_seconds, o.refresh_retry_seconds), (90, 180));
        assert!(o.no_images && o.no_copy_on_click);
        let (mut o, m) = options(&[
            "jirust",
            "--project",
            "NEW",
            "--prefix",
            "NEW",
            "--scope-name",
            "New scope",
            "--view",
            "board",
            "--browse-url",
            "https://other.example.org",
            "--refresh-seconds",
            "300",
            "--refresh-retry-seconds",
            "600",
            "--images",
            "--copy-on-click",
        ]);
        o.apply_environment(&e, &m).unwrap();
        o.validate().unwrap();
        o.apply_scope(&mut s);
        assert_eq!(s.prefix(), "NEW");
        assert_eq!(s.scope_name, "New scope");
        assert_eq!(s.last_view, jirust::config::BOARD);
        assert!(!o.no_images && !o.no_copy_on_click);
        assert_eq!((o.refresh_seconds, o.refresh_retry_seconds), (300, 600));
        assert_eq!(o.browse_url.as_deref(), Some("https://other.example.org"));
        let (mut o, m) = options(&["jirust", "--project", "NEW"]);
        o.apply_environment(&e, &m).unwrap();
        o.apply_scope(&mut s);
        assert_eq!(s.prefix(), "NEW");
        assert!(s.scope_name.is_empty());
    }
    #[test]
    fn unsafe_configuration_is_redacted_and_demo_ignores_personal_environment() {
        for text in [
            "JIRA_VIEW=TOP_SECRET",
            "JIRA_REFRESH_SECONDS=TOP_SECRET",
            "JIRA_REFRESH_SECONDS=1",
            "JIRA_REFRESH_RETRY_SECONDS=86401",
            "JIRA_NO_IMAGES=TOP_SECRET",
            "JIRA_NO_COPY_ON_CLICK=TOP_SECRET",
            "JIRA_BROWSE_URL=https://alex:TOP_SECRET@example.org",
            "JIRA_PREFIX='TOP_SECRET!'",
            "JIRA_SCOPE_NAME=\"a\\nb\"",
        ] {
            let e = env(text);
            let (mut o, m) = options(&["jirust"]);
            let result = o.apply_environment(&e, &m).and_then(|_| o.validate());
            assert!(result.is_err(), "{text}");
            assert!(!result.unwrap_err().to_string().contains("TOP_SECRET"));
        }
        let e = env(
            "JIRA_VIEW=TOP_SECRET\nJIRA_REFRESH_SECONDS=1\nJIRA_BROWSE_URL=https://alex:TOP_SECRET@example.org\nJIRA_NO_IMAGES=true\nJIRA_NO_COPY_ON_CLICK=true",
        );
        let (mut o, m) = options(&["jirust", "--demo"]);
        o.apply_environment(&e, &m).unwrap();
        o.validate().unwrap();
        assert_eq!(o.refresh_seconds, 60);
        assert!(!o.no_images && !o.no_copy_on_click && o.browse_url.is_none());
        for flag in ["--refresh-seconds", "--refresh-retry-seconds"] {
            assert!(
                Options::command()
                    .try_get_matches_from(["jirust", flag, "0"])
                    .is_err()
            );
        }
    }
    #[test]
    fn move_parses_one_key_destination_and_global_read_only() {
        let (o, _) = options(&[
            "jirust",
            "move",
            "DEV-102",
            "--to",
            "board",
            "--yes",
            "--read-only",
            "--board",
            "17",
        ]);
        assert!(o.read_only);
        assert_eq!(o.board, Some(17));
        assert!(
            matches!(o.command,Some(CliCommand::Move{ref key,to:Destination::Board,yes:true}) if key=="DEV-102")
        );
        assert!(
            Options::command()
                .try_get_matches_from(["jirust", "move", "DEV-102", "DEV-103", "--to", "board"])
                .is_err()
        );
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn cancelling_background_detail_releases_the_cli_slot_and_foreground_reads_still_complete()
     {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("fake-acli");
        std::fs::write(&p,"#!/bin/sh\nif [ \"$4\" = DEV-101 ]; then exec /bin/sleep 10; fi\nprintf '%s' '{\"key\":\"DEV-103\",\"fields\":{\"summary\":\"Synthetic\"}}'\n").unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700)).unwrap();
        let backend = Backend::new("acli", Some(p)).unwrap();
        let (cancel, rx) = tokio::sync::watch::channel(false);
        let worker = backend.clone();
        let read = tokio::spawn(async move { detail_read(&worker, "DEV-101", Some(rx)).await });
        tokio::time::sleep(Duration::from_millis(40)).await;
        cancel.send_replace(true);
        assert!(
            tokio::time::timeout(Duration::from_secs(1), read)
                .await
                .unwrap()
                .unwrap()
                .is_err()
        );
        assert_eq!(
            tokio::time::timeout(
                Duration::from_secs(1),
                detail_read(&backend, "DEV-103", None)
            )
            .await
            .unwrap()
            .unwrap()
            .key,
            "DEV-103"
        );
    }
    #[test]
    fn explicit_flags_override_env_and_new_project_does_not_inherit_old_scope() {
        let e = env(
            "JIRA_PROJECT=DEV\nJIRA_PREFIX=OLD\nJIRA_BOARD_ID=17\nJIRA_JQL='project = DEV'\nJIRA_READ_ONLY=false\n",
        );
        let (mut o, m) = options(&["jirust", "--project", "NEW", "--read-only"]);
        o.apply_environment(&e, &m).unwrap();
        assert!(o.read_only);
        assert_eq!(o.project.as_deref(), Some("NEW"));
        assert!(o.prefix.is_none());
        assert!(o.board.is_none());
        assert!(o.jql.is_none());
        let (mut o, m) = options(&["jirust"]);
        o.apply_environment(&e, &m).unwrap();
        assert_eq!(o.project.as_deref(), Some("DEV"));
        assert_eq!(o.board, Some(17));
    }
    #[test]
    fn demo_does_not_apply_company_env_and_read_only_env_cannot_be_bypassed_by_yes() {
        let e = env("JIRA_PROJECT=OTHER\nJIRA_BOARD_ID=17\nJIRA_READ_ONLY=true\n");
        let (mut o, m) = options(&["jirust", "--demo"]);
        o.apply_environment(&e, &m).unwrap();
        assert!(o.project.is_none());
        assert!(o.board.is_none());
        let (mut o, m) = options(&["jirust", "move", "DEV-102", "--to", "board", "--yes"]);
        o.apply_environment(&e, &m).unwrap();
        assert!(o.read_only);
    }
}
