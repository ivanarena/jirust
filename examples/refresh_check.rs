//! Read-only live refresh timings. No settings/cache writes or Jira mutations.
//! cargo run --release --example refresh_check
use anyhow::{Context, Result};
use jirust::{
    backend::Backend,
    config::{BACKLOG, MINE, Settings},
    environment::Environment,
};
use std::time::Instant;
#[tokio::main]
async fn main() -> Result<()> {
    let env = Environment::load(None)?;
    let backend = Backend::new(
        &env.value("JIRA_BACKEND").unwrap_or_else(|| "acli".into()),
        env.value("JIRA_CLI_BIN").map(Into::into),
    )?;
    backend.configure_environment(env.credentials()?)?;
    let mut settings = Settings::load()?;
    if let Some(v) = env.value("JIRA_PROJECT") {
        if settings.project != v.to_uppercase() {
            settings.board_id = None;
            settings.scope_jql.clear();
        }
        settings.project = v.to_uppercase();
    }
    if let Some(v) = env.value("JIRA_PREFIX") {
        settings.prefix = v.to_uppercase();
    }
    if let Some(v) = env.value("JIRA_JQL") {
        settings.scope_jql = v;
    }
    if let Some(v) = env.number("JIRA_BOARD_ID", u64::MAX)? {
        settings.board_id = Some(v);
    }
    settings
        .board_id
        .context("Configure one authorized board first")?;
    backend.configure_board(settings.board_id)?;
    let base = settings.query(BACKLOG, "", "", "")?;
    let mine = settings.query(MINE, "", "", "")?;
    for n in 0..3 {
        let start = Instant::now();
        let (w, _) = backend
            .workspace_progress(&base, &mine, None, |stage| {
                println!(
                    "pass={} phase={} at={:.3}s",
                    n + 1,
                    stage.label(),
                    start.elapsed().as_secs_f64()
                )
            })
            .await?;
        println!(
            "pass={} workspace={:.3}s tickets={} (read-only; no cache writes)",
            n + 1,
            start.elapsed().as_secs_f64(),
            w.items.len()
        );
    }
    Ok(())
}
