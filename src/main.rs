use anyhow::{Result, bail};
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand, parser::ValueSource};
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
    /// Private local dotenv file (default: ./.env, then user config directory).
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
    #[arg(long,default_value_t=500,value_parser=clap::value_parser!(u16).range(1..=2000))]
    limit: u16,
    #[arg(long)]
    check: bool,
    #[arg(long)]
    attachment_dir: Option<PathBuf>,
    #[arg(long,default_value="auto",value_parser=["auto","kitty","halfblocks"])]
    image_protocol: String,
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
        ] {
            if !supplied(name)
                && !(project_changed && name != "project")
                && let Some(v) = env.value(key)
            {
                *field = Some(v);
            }
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
        Environment::load(opts.env_file.as_deref())?
    };
    opts.apply_environment(&env, &matches)?;
    if opts.check && opts.command.is_some() {
        bail!("--check cannot be combined with a command; no operation performed");
    }
    let backend = Backend::new(if opts.demo { "demo" } else { &opts.backend }, opts.cli_bin)?;
    let credentials = if backend.kind == Kind::Demo {
        None
    } else {
        env.credentials()?
    };
    if matches!(opts.command, Some(CliCommand::Auth)) {
        let credentials = credentials.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Set JIRA_URL, JIRA_EMAIL and JIRA_API_TOKEN before authentication")
        })?;
        backend.authenticate(credentials).await?;
        println!("acli authenticated from local environment; token not displayed");
        return Ok(());
    }
    backend.configure_environment(credentials)?;
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
    let scope_override = opts.project.is_some()
        || opts.prefix.is_some()
        || opts.jql.is_some()
        || opts.board.is_some();
    if let Some(project) = opts.project {
        let project = project.to_uppercase();
        if settings.project != project {
            settings.board_id = None;
        }
        settings.project = project;
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
    if let Some(id) = opts.board {
        settings.board_id = Some(id)
    };
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
        let items = workspace.items;
        if let Some(t) = items.first() {
            backend.get(&t.key).await?;
        };
        println!(
            "backend={} search=ok tickets={} detail={} mine={} available={} board={} backlog={} epics={} (read-only; no records changed)",
            backend.kind.name(),
            items.len(),
            if items.is_empty() { "skipped" } else { "ok" },
            counts[0],
            counts[1],
            counts[2],
            counts[3],
            epics
        );
        return Ok(());
    }
    if scope_override && backend.kind != Kind::Demo {
        settings.save()?;
    }
    if !io::stdin().is_terminal() {
        bail!("Run jirust in a terminal, or use --check")
    }
    let scope = backend.scope().await;
    let cache = Cache::new(&scope, opts.no_cache || backend.kind == Kind::Demo)?;
    let attachment_dir = opts
        .attachment_dir
        .unwrap_or(jirust::config::attachment_directory(&scope)?);
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
#[cfg(test)]
mod tests {
    use super::*;
    fn options(args: &[&str]) -> (Options, clap::ArgMatches) {
        let matches = Options::command().try_get_matches_from(args).unwrap();
        (Options::from_arg_matches(&matches).unwrap(), matches)
    }
    fn env(text: &str) -> Environment {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(text.as_bytes()).unwrap();
        Environment::load(Some(f.path())).unwrap()
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
