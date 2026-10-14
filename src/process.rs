//! Bounded subprocess capture. Never expose raw output through an error chain.
use anyhow::{Context, Result, bail};
use std::process::{ExitStatus, Stdio};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
};

async fn read<R: AsyncRead + Unpin>(mut input: R, limit: usize, retain: bool) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut count = 0usize;
    // Keep buffers off the async future's stack (nested workspace joins).
    let mut chunk = vec![0u8; 8192];
    loop {
        let n = input
            .read(&mut chunk)
            .await
            .context("Cannot read subprocess output")?;
        if n == 0 {
            return Ok(bytes);
        }
        if n > limit.saturating_sub(count) {
            bail!(
                "Subprocess output exceeds limit; update outcome may be unknown. Reload before retrying."
            );
        }
        count += n;
        if retain {
            bytes.extend_from_slice(&chunk[..n]);
        }
    }
}
pub(crate) async fn output(
    mut command: Command,
    stdout_limit: usize,
    stderr_limit: usize,
) -> Result<(ExitStatus, Vec<u8>)> {
    command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn().context("Cannot start subprocess")?;
    let stdout = child
        .stdout
        .take()
        .context("Subprocess stdout unavailable")?;
    let stderr = child
        .stderr
        .take()
        .context("Subprocess stderr unavailable")?;
    let result = tokio::try_join!(
        read(stdout, stdout_limit, true),
        read(stderr, stderr_limit, false),
        async { child.wait().await.context("Cannot wait for subprocess") }
    );
    match result {
        Ok((bytes, _, status)) => Ok((status, bytes)),
        Err(error) => {
            let _ = child.kill().await;
            Err(error)
        }
    }
}
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    fn command(script: &str) -> Command {
        let mut c = Command::new("/bin/sh");
        c.args(["-c", script]);
        c
    }
    #[tokio::test]
    async fn captures_output_without_retaining_diagnostic_secrets() {
        let (status, bytes) = output(
            command("printf fixture; printf 'Authorization: Bearer SYNTHETIC_ONLY' >&2; exit 7"),
            100,
            100,
        )
        .await
        .unwrap();
        assert_eq!(status.code(), Some(7));
        assert_eq!(bytes, b"fixture");
    }
    #[tokio::test]
    async fn both_streams_are_bounded_and_overflow_kills_child() {
        for script in [
            "while :; do printf '12345678'; done",
            "while :; do printf '12345678' >&2; done",
        ] {
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(3),
                output(command(script), 1024, 1024),
            )
            .await
            .unwrap();
            assert!(result.unwrap_err().to_string().contains("exceeds limit"));
        }
    }
}
