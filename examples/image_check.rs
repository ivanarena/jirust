//! Explicit read-only image diagnostic; no terminal graphics or cache writes.
//! cargo run --example image_check -- DEV-123
use anyhow::{Context, Result};
use jirust::{backend::Backend, data, environment, images};
#[tokio::main]
async fn main() -> Result<()> {
    let key = std::env::args().nth(1).context("Provide one ticket key")?;
    data::require_key(&key)?;
    let env = environment::Environment::load(None)?;
    let backend = Backend::new("acli", env.value("JIRA_CLI_BIN").map(Into::into))?;
    backend.configure_environment(env.credentials()?)?;
    backend.configure_board(None)?;
    let ticket = backend.get(&key).await?;
    let attachment = ticket
        .attachments()
        .into_iter()
        .find(|a| images::validate_attachment(a).is_ok())
        .context("No supported bounded raster attachments on this ticket")?;
    let bytes = backend.image_bytes(&key, &attachment, None).await?;
    let image = images::decode(&bytes)?;
    println!(
        "image=ok bytes={} dimensions={}×{} (read-only; no image cache or Jira writes)",
        bytes.len(),
        image.width(),
        image.height()
    );
    Ok(())
}
