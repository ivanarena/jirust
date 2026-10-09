use anyhow::{Result, bail};
use clap::Parser;
use crossterm::{
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use jiraui::{
    app::{App, Job, Message},
    backend::{Backend, Kind},
    config::{Cache, Settings},
    data, render,
};
use ratatui::{Terminal, backend::CrosstermBackend, layout::Size};
use ratatui_image::{
    Resize,
    picker::{Picker, ProtocolType},
};
use std::{
    io::{self, IsTerminal},
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
    #[arg(long, default_value = "auto")]
    backend: String,
    #[arg(long)]
    demo: bool,
    #[arg(long)]
    cli_bin: Option<PathBuf>,
    #[arg(long)]
    read_only: bool,
    #[arg(long)]
    no_cache: bool,
    #[arg(long)]
    project: Option<String>,
    #[arg(long)]
    prefix: Option<String>,
    #[arg(long)]
    jql: Option<String>,
    #[arg(long,default_value_t=500,value_parser=clap::value_parser!(u16).range(1..=2000))]
    limit: u16,
    #[arg(long)]
    check: bool,
    #[arg(long)]
    attachment_dir: Option<PathBuf>,
    #[arg(long,default_value="auto",value_parser=["auto","kitty","halfblocks"])]
    image_protocol: String,
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
    let opts = Options::parse();
    let backend = Backend::new(if opts.demo { "demo" } else { &opts.backend }, opts.cli_bin)?;
    let mut settings = if backend.kind == Kind::Demo {
        Settings {
            project: "DEV".into(),
            last_view: jiraui::config::MINE,
            ..Default::default()
        }
    } else {
        Settings::load()?
    };
    let scope_override = opts.project.is_some() || opts.prefix.is_some() || opts.jql.is_some();
    if let Some(project) = opts.project {
        settings.project = project.to_uppercase();
        if opts.prefix.is_none() {
            settings.prefix.clear();
        }
        if opts.jql.is_none() {
            settings.scope_jql.clear();
        }
    };
    if let Some(prefix) = opts.prefix {
        settings.prefix = prefix.to_uppercase()
    };
    if let Some(jql) = opts.jql {
        settings.scope_jql = jql
    };
    let limit = if backend.kind == Kind::Jira {
        usize::from(opts.limit).min(100)
    } else {
        usize::from(opts.limit)
    };
    if opts.check {
        let q = settings.query(2, "", "", "")?;
        let mine = settings.query(jiraui::config::MINE, "", "", "")?;
        let (workspace, _) = backend.workspace(&q, &mine, None).await?;
        let items = workspace.items;
        if let Some(t) = items.first() {
            backend.get(&t.key).await?;
        };
        println!(
            "backend={} search=ok tickets={} detail={} (read-only; no records changed)",
            backend.kind.name(),
            items.len(),
            if items.is_empty() { "skipped" } else { "ok" }
        );
        return Ok(());
    }
    if scope_override && backend.kind != Kind::Demo {
        settings.save()?;
    }
    if !io::stdin().is_terminal() {
        bail!("Run jiraui in a terminal, or use --check")
    }
    let scope = backend.scope().await;
    let cache = Cache::new(&scope, opts.no_cache || backend.kind == Kind::Demo)?;
    let attachment_dir = opts
        .attachment_dir
        .unwrap_or(jiraui::config::attachment_directory(&scope)?);
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
    let mut app = App::new(
        backend.clone(),
        cache,
        settings,
        opts.read_only,
        limit,
        attachment_dir,
    );
    let jobs = app.reload();
    dispatch(jobs, &backend, &picker, &tx, &app.settings.project);
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
            dispatch(jobs, &backend, &picker, &tx, &app.settings.project);
            dirty = true;
        }
        if app.quit {
            break;
        }
        for _ in 0..8 {
            if event::poll(Duration::ZERO)? {
                break;
            }
            let Ok(message) = rx.try_recv() else { break };
            let jobs = app.message(message);
            dispatch(jobs, &backend, &picker, &tx, &app.settings.project);
            dirty = true;
        }
        let jobs = app.advance();
        if !jobs.is_empty() {
            dispatch(jobs, &backend, &picker, &tx, &app.settings.project);
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
            dispatch(jobs, &backend, &picker, &tx, &app.settings.project);
            dirty = true
        }
    }
    drop(terminal);
    drop(guard);
    Ok(())
}
fn dispatch(
    jobs: Vec<Job>,
    backend: &Arc<Backend>,
    picker: &Picker,
    tx: &mpsc::UnboundedSender<Message>,
    _project: &str,
) {
    for job in jobs {
        let backend = backend.clone();
        let tx = tx.clone();
        let picker = picker.clone();
        tokio::spawn(async move {
            let message = match job {
                Job::Sync {
                    generation,
                    base,
                    mine,
                    changed,
                } => Message::Synced {
                    generation,
                    result: backend.workspace(&base, &mine, changed.as_deref()).await,
                },

                Job::Detail {
                    key,
                    lookup,
                    revision,
                    background,
                } => Message::Detail {
                    key: key.clone(),
                    lookup,
                    revision,
                    background,
                    result: backend.get(&key).await,
                },
                Job::Comments(key) => Message::Comments {
                    key: key.clone(),
                    result: backend.comments(&key).await,
                },
                Job::Write(action) => Message::Written {
                    action: action.clone(),
                    result: backend.apply(&action).await,
                },
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
                Job::Preview {
                    generation,
                    path,
                    size,
                } => {
                    let name = path
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "image".into());
                    let result = tokio::task::spawn_blocking(move || {
                        let metadata = std::fs::metadata(&path)?;
                        if metadata.len() > 16 * 1024 * 1024 {
                            bail!("Image is larger than 16 MiB")
                        };
                        let dimensions = image::ImageReader::open(&path)?
                            .with_guessed_format()?
                            .into_dimensions()?;
                        if u64::from(dimensions.0) * u64::from(dimensions.1) > 12_000_000 {
                            bail!("Image exceeds 12 million pixels")
                        };
                        let mut reader = image::ImageReader::open(path)?.with_guessed_format()?;
                        let mut limits = image::Limits::default();
                        limits.max_alloc = Some(64 * 1024 * 1024);
                        reader.limits(limits);
                        let image = reader.decode()?;
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
