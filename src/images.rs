//! Bounded raster images, private ID-keyed caches, and a redirect-free reader.
use crate::{
    config,
    data::{self, Attachment},
};
use anyhow::{Result, ensure};
use image::{DynamicImage, ImageFormat, ImageReader};
use std::{
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

pub const MAX_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_PIXELS: u64 = 12_000_000;
pub const CACHE_BYTES: u64 = 128 * 1024 * 1024;
pub const MAX_INLINE: usize = 8;
#[derive(Clone)]
pub struct ImageSlot {
    pub row: usize,
    pub height: u16,
    pub attachment: Attachment,
}
pub fn insert_rows(
    lines: &mut Vec<crate::markdown::DocLine>,
    ticket: &data::Ticket,
    height: u16,
) -> Vec<ImageSlot> {
    use crate::markdown::{Chunk, DocLine};
    use ratatui::style::{Color, Style};
    let attachments = ticket
        .attachments()
        .into_iter()
        .filter(|a| validate_attachment(a).is_ok())
        .collect::<Vec<_>>();
    if attachments.is_empty() {
        return vec![];
    }
    let start = lines
        .iter()
        .position(|l| l.chunks.iter().any(|c| c.text == "Links & attachments"))
        .unwrap_or(lines.len());
    let mut rows = vec![DocLine {
        chunks: vec![Chunk {
            text: "Images (inline attachments)".into(),
            style: Style::default().fg(Color::Blue),
            target: None,
        }],
    }];
    let mut slots = vec![];
    for a in attachments.iter().take(MAX_INLINE) {
        rows.push(DocLine {
            chunks: vec![Chunk {
                text: format!("▧ {}", data::line(&a.name)),
                style: Style::default().fg(Color::Blue),
                target: Some(data::Target {
                    label: a.name.clone(),
                    attachment: Some(a.clone()),
                    ..Default::default()
                }),
            }],
        });
        slots.push(ImageSlot {
            row: start + rows.len(),
            height,
            attachment: a.clone(),
        });
        rows.extend((0..height).map(|_| DocLine::default()));
        rows.push(DocLine::default());
    }
    if attachments.len() > MAX_INLINE {
        rows.push(DocLine {
            chunks: vec![Chunk {
                text: "More images: l opens the attachment list".into(),
                style: Style::default().fg(Color::DarkGray),
                target: None,
            }],
        });
    }
    lines.splice(start..start, rows);
    slots
}
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 20
        && id.bytes().all(|b| b.is_ascii_digit())
        && id.parse::<u64>().is_ok_and(|n| n > 0)
}
pub fn validate_attachment(a: &Attachment) -> Result<()> {
    ensure!(valid_id(&a.id), "Invalid image attachment ID");
    ensure!(
        a.is_image(),
        "Only PNG, JPEG, GIF and WebP attachments render in-terminal"
    );
    ensure!(a.size <= MAX_BYTES, "Image is larger than 16 MiB");
    Ok(())
}
pub async fn response_bytes(mut response: reqwest::Response) -> Result<Vec<u8>> {
    ensure!(
        !response.status().is_redirection(),
        "Jira redirected the image request; blocked to protect credentials. Download it through Jira manually."
    );
    ensure!(
        response.status().as_u16() == 200,
        "Image download HTTP {} (details redacted)",
        response.status().as_u16()
    );
    let mime = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_lowercase();
    ensure!(
        matches!(
            mime.as_str(),
            "image/png"
                | "image/jpeg"
                | "image/jpg"
                | "image/gif"
                | "image/webp"
                | "application/octet-stream"
        ),
        "Image response has an unsupported content type"
    );
    ensure!(
        response.content_length().is_none_or(|n| n <= MAX_BYTES),
        "Image is larger than 16 MiB"
    );
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| anyhow::anyhow!("Image response interrupted"))?
    {
        ensure!(
            bytes.len() as u64 + chunk.len() as u64 <= MAX_BYTES,
            "Image exceeds the 16 MiB download limit"
        );
        bytes.extend_from_slice(&chunk);
    }
    ensure!(!bytes.is_empty(), "Image response is empty");
    Ok(bytes)
}
pub fn decode(bytes: &[u8]) -> Result<DynamicImage> {
    ensure!(
        bytes.len() as u64 <= MAX_BYTES,
        "Image is larger than 16 MiB"
    );
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| anyhow::anyhow!("Invalid image header"))?;
    ensure!(
        matches!(
            reader.format(),
            Some(ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::Gif | ImageFormat::WebP)
        ),
        "Unsupported image format (SVG and external media are not rendered)"
    );
    let (w, h) = reader
        .into_dimensions()
        .map_err(|_| anyhow::anyhow!("Invalid image dimensions"))?;
    ensure!(
        w > 0 && h > 0 && u64::from(w) * u64::from(h) <= MAX_PIXELS,
        "Image exceeds 12 million pixels"
    );
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| anyhow::anyhow!("Invalid image header"))?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    reader
        .decode()
        .map_err(|_| anyhow::anyhow!("Invalid or oversized raster image"))
}
pub fn local_bytes(path: &Path) -> Result<Vec<u8>> {
    let metadata =
        std::fs::metadata(path).map_err(|_| anyhow::anyhow!("Cannot open local image"))?;
    ensure!(
        metadata.is_file() && metadata.len() <= MAX_BYTES,
        "Image is not a regular file or exceeds 16 MiB"
    );
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= MAX_BYTES, "Image exceeds 16 MiB");
    Ok(bytes)
}
pub fn cache_path(root: Option<&Path>, key: &str, id: &str) -> Option<PathBuf> {
    if !data::valid_key(key) || !valid_id(id) {
        return None;
    }
    let dir = root?.join("images-v1");
    if let Ok(m) = std::fs::symlink_metadata(&dir)
        && (!m.is_dir() || m.file_type().is_symlink())
    {
        return None;
    }
    Some(dir.join(format!("{key}-{id}.bin")))
}
pub fn cached_bytes(path: &Path) -> Option<Vec<u8>> {
    let m = std::fs::symlink_metadata(path).ok()?;
    if !m.is_file() || m.file_type().is_symlink() || m.len() > MAX_BYTES {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if m.permissions().mode() & 0o077 != 0 {
            return None;
        }
    }
    local_bytes(path).ok()
}
pub fn store(path: &Path, bytes: &[u8]) -> Result<()> {
    store_with_quota(path, bytes, CACHE_BYTES)
}
fn store_with_quota(path: &Path, bytes: &[u8], quota: u64) -> Result<()> {
    ensure!(
        bytes.len() as u64 <= MAX_BYTES,
        "Image exceeds cache entry limit"
    );
    let dir = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Missing image cache directory"))?;
    if let Ok(m) = std::fs::symlink_metadata(dir) {
        ensure!(
            m.is_dir() && !m.file_type().is_symlink(),
            "Image cache must be a regular directory"
        );
    }
    let mut entries = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            let stem = p.file_stem()?.to_str()?;
            let (key, id) = stem.rsplit_once('-')?;
            if p == path
                || p.extension()?.to_str()? != "bin"
                || !data::valid_key(key)
                || !valid_id(id)
            {
                return None;
            }
            let m = std::fs::symlink_metadata(&p).ok()?;
            if !m.is_file() || m.file_type().is_symlink() {
                return None;
            }
            Some((m.modified().ok(), m.len(), p))
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|e| e.0);
    let mut total = entries
        .iter()
        .fold(0u64, |total, e| total.saturating_add(e.1));
    for (_, size, p) in entries {
        if total.saturating_add(bytes.len() as u64) <= quota {
            break;
        }
        if std::fs::remove_file(p).is_ok() {
            total = total.saturating_sub(size);
        }
    }
    ensure!(
        total.saturating_add(bytes.len() as u64) <= quota,
        "Image cache quota reached"
    );
    config::private_write(path, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn png() -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        DynamicImage::new_rgb8(2, 2)
            .write_to(&mut out, ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }
    #[test]
    fn decoding_and_cache_are_bounded_private_and_id_keyed() {
        let bytes = png();
        assert_eq!(decode(&bytes).unwrap().width(), 2);
        assert!(decode(b"<svg>external</svg>").is_err());
        assert!(decode(&vec![0; MAX_BYTES as usize + 1]).is_err());
        let dir = tempfile::tempdir().unwrap();
        let p = cache_path(Some(dir.path()), "DEV-1", "23").unwrap();
        store(&p, &bytes).unwrap();
        assert_eq!(cached_bytes(&p).unwrap(), bytes);
        assert_ne!(p, cache_path(Some(dir.path()), "DEV-1", "24").unwrap());
        assert!(cache_path(Some(dir.path()), "../DEV-1", "23").is_none());
        assert!(cache_path(Some(dir.path()), "DEV-1", "23/../../").is_none());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(p).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
    #[test]
    fn cache_quota_evicts_owned_entries_and_never_follows_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let first = cache_path(Some(dir.path()), "DEV-1", "23").unwrap();
        let second = cache_path(Some(dir.path()), "DEV-1", "24").unwrap();
        store_with_quota(&first, b"synthetic-first", 20).unwrap();
        store_with_quota(&second, b"synthetic-next", 20).unwrap();
        assert!(!first.exists());
        assert!(second.exists());
        #[cfg(unix)]
        {
            let link = cache_path(Some(dir.path()), "DEV-1", "25").unwrap();
            std::os::unix::fs::symlink(&second, &link).unwrap();
            assert!(cached_bytes(&link).is_none());
        }
    }
    #[test]
    fn validation_rejects_external_or_non_raster_identifiers() {
        for id in [
            "",
            "0",
            "../2",
            "https://example.invalid/image",
            "2?token=synthetic",
        ] {
            assert!(!valid_id(id));
        }
        let mut a = Attachment {
            id: "23".into(),
            mime: "image/png".into(),
            ..Default::default()
        };
        assert!(validate_attachment(&a).is_ok());
        a.mime = "image/svg+xml".into();
        assert!(validate_attachment(&a).is_err());
        a.mime = "image/png".into();
        a.size = MAX_BYTES + 1;
        assert!(validate_attachment(&a).is_err());
    }
    async fn response(status: &str, headers: &str, body: &[u8]) -> reqwest::Response {
        use std::io::Write;
        let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = socket.local_addr().unwrap();
        let data =
            format!("HTTP/1.1 {status}\r\n{headers}\r\nConnection: close\r\n\r\n").into_bytes();
        let body = body.to_vec();
        std::thread::spawn(move || {
            let (mut stream, _) = socket.accept().unwrap();
            let mut request = [0; 4096];
            let _ = stream.read(&mut request);
            stream.write_all(&data).unwrap();
            let _ = stream.write_all(&body);
        });
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap()
            .get(format!("http://{addr}"))
            .send()
            .await
            .unwrap()
    }
    #[tokio::test]
    async fn redirects_html_and_oversize_responses_fail_without_exposing_signed_urls() {
        let r = response(
            "303 See Other",
            "Location: https://example.invalid/?token=synthetic-secret\r\nContent-Length: 0",
            b"",
        )
        .await;
        let e = response_bytes(r).await.unwrap_err().to_string();
        assert!(!e.contains("synthetic-secret"));
        assert!(
            response_bytes(
                response(
                    "200 OK",
                    "Content-Type: text/html\r\nContent-Length: 1",
                    b"x"
                )
                .await
            )
            .await
            .is_err()
        );
        assert!(
            response_bytes(
                response(
                    "200 OK",
                    &format!(
                        "Content-Type: image/png\r\nContent-Length: {}",
                        MAX_BYTES + 1
                    ),
                    b""
                )
                .await
            )
            .await
            .is_err()
        );
        let bytes = png();
        assert_eq!(
            response_bytes(
                response(
                    "200 OK",
                    &format!("Content-Type: image/png\r\nContent-Length: {}", bytes.len()),
                    &bytes
                )
                .await
            )
            .await
            .unwrap(),
            bytes
        );
    }
}
