//! Explicit ticket-link copies: stdin-only native helpers, or generated OSC52.
//! Writes are serialized/coalesced so a slower earlier click cannot win last.
use anyhow::{Result, ensure};
use std::{path::PathBuf, process::Stdio, time::Duration};
use tokio::{io::AsyncWriteExt, process::Command, sync::watch};
#[derive(Clone)]
pub struct Request {
    pub generation: u64,
    pub url: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Native,
    Terminal,
}
pub struct Completion {
    pub request: Request,
    pub result: Result<Method>,
}
pub fn validate(url: &str) -> Result<()> {
    ensure!(
        url.len() <= 8192 && crate::data::valid_url(url),
        "Ticket has no safe clipboard link"
    );
    let u = url::Url::parse(url)?;
    ensure!(
        u.username().is_empty()
            && u.password().is_none()
            && u.query().is_none()
            && u.fragment().is_none()
            && u.path()
                .strip_prefix("/browse/")
                .is_some_and(crate::data::valid_key),
        "Ticket has no safe clipboard link"
    );
    Ok(())
}
fn find(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|v| std::env::split_paths(&v).collect::<Vec<_>>())
        .map(|p| p.join(name))
        .find(|p| p.is_file())
}
fn helper() -> Option<(PathBuf, Vec<&'static str>)> {
    if std::env::var_os("SSH_CONNECTION").is_some() || std::env::var_os("SSH_TTY").is_some() {
        return None;
    }
    if cfg!(target_os = "macos") {
        return Some((PathBuf::from("/usr/bin/pbcopy"), vec![]));
    }
    if std::env::var_os("WAYLAND_DISPLAY").is_some()
        && let Some(p) = find("wl-copy")
    {
        return Some((p, vec!["--type", "text/plain"]));
    }
    if std::env::var_os("DISPLAY").is_some() {
        if let Some(p) = find("xclip") {
            return Some((p, vec!["-selection", "clipboard", "-in"]));
        }
        if let Some(p) = find("xsel") {
            return Some((p, vec!["--clipboard", "--input"]));
        }
    }
    None
}
async fn native(path: &std::path::Path, args: &[&str], url: &str) -> Result<()> {
    validate(url)?;
    tokio::time::timeout(Duration::from_secs(2), async {
        let mut child = Command::new(path)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| anyhow::anyhow!("Clipboard helper unavailable"))?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("Clipboard input unavailable"))?;
        stdin
            .write_all(url.as_bytes())
            .await
            .map_err(|_| anyhow::anyhow!("Clipboard input failed"))?;
        stdin
            .shutdown()
            .await
            .map_err(|_| anyhow::anyhow!("Clipboard input failed"))?;
        drop(stdin);
        ensure!(
            child
                .wait()
                .await
                .map_err(|_| anyhow::anyhow!("Clipboard helper failed"))?
                .success(),
            "Clipboard helper failed"
        );
        Ok(())
    })
    .await
    .map_err(|_| anyhow::anyhow!("Clipboard helper timed out"))?
}
pub async fn copy(url: &str) -> Result<Method> {
    validate(url)?;
    if let Some((path, args)) = helper()
        && native(&path, &args, url).await.is_ok()
    {
        return Ok(Method::Native);
    }
    Ok(Method::Terminal)
}
/// UI thread only, to avoid interleaving OSC52 with a frame draw. No clipboard reads.
pub fn terminal(url: &str) -> Result<()> {
    validate(url)?;
    crossterm::execute!(
        std::io::stdout(),
        crossterm::clipboard::CopyToClipboard::to_clipboard_from(url)
    )
    .map_err(|_| anyhow::anyhow!("Terminal clipboard request failed"))?;
    Ok(())
}
pub async fn run(rx: watch::Receiver<Option<Request>>, send: impl Fn(Completion)) {
    run_with(rx, |url| async move { copy(&url).await }, send).await
}
async fn run_with<F, Fut>(
    mut rx: watch::Receiver<Option<Request>>,
    copy: F,
    send: impl Fn(Completion),
) where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = Result<Method>>,
{
    let mut last = None;
    loop {
        let request = rx.borrow_and_update().clone();
        if let Some(request) = request
            && Some(request.generation) != last
        {
            last = Some(request.generation);
            let result = copy(request.url.clone()).await;
            send(Completion { request, result });
        } else if rx.changed().await.is_err() {
            break;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_safe_browse_links_are_copyable_and_osc52_encodes_text() {
        let url = "https://example.atlassian.net/browse/DEV-123";
        assert!(validate(url).is_ok());
        for bad in [
            "javascript:alert(1)",
            "https://user:secret@example.invalid/browse/DEV-1",
            "https://example.invalid/browse/DEV-1?token=secret",
            "https://example.invalid/browse/DEV-1\u{1b}]52;c;bad\u{7}",
            "https://example.invalid/rest/api/3/issue/1",
        ] {
            assert!(validate(bad).is_err());
        }
        use crossterm::Command;
        let mut encoded = String::new();
        crossterm::clipboard::CopyToClipboard::to_clipboard_from(url)
            .write_ansi(&mut encoded)
            .unwrap();
        assert!(encoded.starts_with("\u{1b}]52;c;"));
        assert!(!encoded.contains(url));
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn native_copy_sends_link_via_stdin_never_argv_and_redacts_errors() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("copy");
        let input = dir.path().join("input");
        let argv = dir.path().join("argv");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nprintf '%s' \"$*\" > '{}'\ncat > '{}'\n",
                argv.display(),
                input.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let url = "https://example.atlassian.net/browse/DEV-123";
        native(&script, &["--fake"], url).await.unwrap();
        assert_eq!(std::fs::read_to_string(input).unwrap(), url);
        assert_eq!(std::fs::read_to_string(argv).unwrap(), "--fake");
        std::fs::write(&script, "#!/bin/sh\necho secret-from-helper >&2\nexit 1\n").unwrap();
        let error = native(&script, &[], url).await.unwrap_err().to_string();
        assert!(!error.contains("secret-from-helper"));
    }
    #[tokio::test]
    async fn rapid_clicks_coalesce_and_last_click_wins_in_order() {
        use std::sync::{Arc, Mutex};
        let writes = Arc::new(Mutex::new(vec![]));
        let gate = Arc::new(tokio::sync::Notify::new());
        let started = Arc::new(tokio::sync::Notify::new());
        let (tx, rx) = watch::channel(None);
        let w = writes.clone();
        let g = gate.clone();
        let s = started.clone();
        let task = tokio::spawn(async move {
            run_with(
                rx,
                move |url| {
                    let w = w.clone();
                    let g = g.clone();
                    let s = s.clone();
                    async move {
                        if url.ends_with("DEV-1") {
                            s.notify_one();
                            g.notified().await;
                        }
                        w.lock().unwrap().push(url);
                        Ok(Method::Native)
                    }
                },
                |_| {},
            )
            .await;
        });
        tx.send_replace(Some(Request {
            generation: 1,
            url: "https://example.invalid/browse/DEV-1".into(),
        }));
        started.notified().await;
        tx.send_replace(Some(Request {
            generation: 2,
            url: "https://example.invalid/browse/DEV-2".into(),
        }));
        tx.send_replace(Some(Request {
            generation: 3,
            url: "https://example.invalid/browse/DEV-3".into(),
        }));
        gate.notify_one();
        drop(tx);
        task.await.unwrap();
        let writes = writes.lock().unwrap();
        assert_eq!(writes.len(), 2);
        assert!(writes[0].ends_with("DEV-1"));
        assert!(writes[1].ends_with("DEV-3"));
    }
}
