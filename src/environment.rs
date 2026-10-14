//! Explicit local configuration; never put credentials in Settings or Debug output.
use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Default)]
pub struct Environment {
    values: BTreeMap<String, String>,
}
#[derive(Clone)]
pub struct Credentials {
    pub(crate) site: String,
    pub(crate) email: String,
    pub(crate) token: String,
}
fn file_present(path: &Path) -> Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(anyhow::anyhow!(
            "Cannot inspect environment file; no fallback profile selected"
        )),
    }
}
fn selected_file(
    explicit: Option<&Path>,
    profile: Option<PathBuf>,
    local: &Path,
    user: impl FnOnce() -> Result<PathBuf>,
) -> Result<Option<PathBuf>> {
    if let Some(path) = explicit.map(Path::to_owned).or(profile) {
        return Ok(Some(path));
    }
    if file_present(local)? {
        return Ok(Some(local.to_owned()));
    }
    let user = user()?;
    Ok(file_present(&user)?.then_some(user))
}
impl Environment {
    pub fn load(explicit: Option<&Path>) -> Result<Self> {
        let profile = std::env::var_os("JIRUST_ENV_FILE")
            .filter(|s| !s.is_empty())
            .map(PathBuf::from);
        // Do not even resolve/migrate the personal config directory for an explicit profile.
        let path = selected_file(explicit, profile, Path::new(".env"), || {
            Ok(crate::config::config_directory()?.join(".env"))
        })?;
        let Some(path) = path else {
            return Ok(Self::default());
        };
        let bytes=crate::private_fs::read_environment(&path,65536).context("Cannot read private environment file; require an owned regular 0600 file, no symlink/hardlink, at most 64 KiB")?;
        let values = dotenvy::from_read_iter(bytes.as_slice())
            .collect::<std::result::Result<BTreeMap<_, _>, _>>()
            .map_err(|_| anyhow::anyhow!("Invalid environment file syntax (values redacted)"))?;
        Ok(Self { values })
    }
    pub fn value(&self, key: &str) -> Option<String> {
        std::env::var(key)
            .ok()
            .or_else(|| self.values.get(key).cloned())
            .filter(|s| !s.is_empty())
    }
    pub fn boolean(&self, key: &str) -> Result<Option<bool>> {
        self.value(key)
            .map(|s| match s.as_str() {
                "true" | "1" => Ok(true),
                "false" | "0" => Ok(false),
                _ => Err(anyhow::anyhow!(
                    "Invalid boolean in {key}; use true or false"
                )),
            })
            .transpose()
    }
    pub fn number(&self, key: &str, max: u64) -> Result<Option<u64>> {
        self.value(key)
            .map(|s| {
                let n = s
                    .parse::<u64>()
                    .map_err(|_| anyhow::anyhow!("Invalid number in {key}"))?;
                ensure!(n > 0 && n <= max, "Out-of-range number in {key}");
                Ok(n)
            })
            .transpose()
    }
    pub fn credentials(&self) -> Result<Option<Arc<Credentials>>> {
        let values = ["JIRA_URL", "JIRA_EMAIL", "JIRA_API_TOKEN"].map(|k| self.value(k));
        if values.iter().all(Option::is_none) {
            return Ok(None);
        }
        ensure!(
            values.iter().all(Option::is_some),
            "Set JIRA_URL, JIRA_EMAIL and JIRA_API_TOKEN together"
        );
        let [Some(url), Some(email), Some(token)] = values else {
            unreachable!()
        };
        let site = cloud_site(&url)?;
        ensure!(
            email.len() <= 254
                && email.contains('@')
                && email.bytes().all(|b| b.is_ascii_graphic()),
            "Invalid JIRA_EMAIL"
        );
        ensure!(
            !token.is_empty()
                && token.len() <= 16384
                && token.bytes().all(|b| b.is_ascii_graphic()),
            "Invalid JIRA_API_TOKEN (value redacted)"
        );
        Ok(Some(Arc::new(Credentials { site, email, token })))
    }
}
fn cloud_site(value: &str) -> Result<String> {
    let url = url::Url::parse(value).map_err(|_| anyhow::anyhow!("Invalid JIRA_URL"))?;
    ensure!(
        url.scheme() == "https"
            && url
                .host_str()
                .is_some_and(|h| h.ends_with(".atlassian.net"))
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none()
            && url.path() == "/"
            && url.query().is_none()
            && url.fragment().is_none(),
        "JIRA_URL must be an HTTPS Atlassian Cloud origin without credentials, port or path"
    );
    Ok(url.host_str().unwrap().to_owned())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    fn env(values: &[(&str, &str)]) -> Environment {
        Environment {
            values: values
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }
    #[test]
    fn profile_selection_is_explicit_then_exported_then_local_then_user() {
        let d = tempfile::tempdir().unwrap();
        let local = d.path().join(".env");
        let user = d.path().join("user.env");
        let profile = d.path().join("profile.env");
        let explicit = d.path().join("explicit.env");
        fs::write(&local, b"").unwrap();
        fs::write(&user, b"").unwrap();
        assert_eq!(
            selected_file(Some(&explicit), Some(profile.clone()), &local, || panic!(
                "must not resolve personal configuration"
            ))
            .unwrap(),
            Some(explicit)
        );
        assert_eq!(
            selected_file(None, Some(profile.clone()), &local, || panic!(
                "must not resolve personal configuration"
            ))
            .unwrap(),
            Some(profile)
        );
        assert_eq!(
            selected_file(None, None, &local, || panic!(
                "must not resolve personal configuration"
            ))
            .unwrap(),
            Some(local.clone())
        );
        fs::remove_file(local).unwrap();
        assert_eq!(
            selected_file(None, None, &d.path().join("missing"), || Ok(user.clone())).unwrap(),
            Some(user)
        );
    }
    #[cfg(unix)]
    #[test]
    fn failed_default_profile_inspection_never_falls_back_to_another_file() {
        use std::os::unix::fs::PermissionsExt;
        if rustix::process::geteuid().as_raw() == 0 {
            return;
        }
        let d = tempfile::tempdir().unwrap();
        let local = d.path().join("blocked");
        fs::create_dir(&local).unwrap();
        fs::write(local.join(".env"), b"TEST_SECRET=synthetic\n").unwrap();
        fs::set_permissions(&local, fs::Permissions::from_mode(0o000)).unwrap();
        let fallback_called = std::cell::Cell::new(false);
        let result = selected_file(None, None, &local.join(".env"), || {
            fallback_called.set(true);
            Ok(d.path().join("other.env"))
        });
        fs::set_permissions(&local, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(result.is_err());
        assert!(!fallback_called.get());
    }
    #[cfg(unix)]
    #[test]
    fn environment_reader_rejects_symlinks_hardlinks_and_oversized_files() {
        use std::io::Write;
        let d = tempfile::tempdir().unwrap();
        let mut f = tempfile::NamedTempFile::new_in(d.path()).unwrap();
        writeln!(f, "TEST_SECRET=synthetic").unwrap();
        let link = f.path().with_extension("link");
        std::os::unix::fs::symlink(f.path(), &link).unwrap();
        assert!(Environment::load(Some(&link)).is_err());
        fs::remove_file(&link).unwrap();
        fs::hard_link(f.path(), &link).unwrap();
        assert!(Environment::load(Some(f.path())).is_err());
        fs::remove_file(&link).unwrap();
        f.as_file_mut().set_len(65537).unwrap();
        assert!(Environment::load(Some(f.path())).is_err());
    }
    #[test]
    fn local_credentials_validate_origin_and_never_debug_values() {
        let e = env(&[
            ("JIRA_URL", "https://example.atlassian.net"),
            ("JIRA_EMAIL", "alex@example.com"),
            ("JIRA_API_TOKEN", "synthetic-token"),
        ]);
        assert_eq!(
            e.credentials().unwrap().unwrap().site,
            "example.atlassian.net"
        );
        for bad in [
            "http://example.atlassian.net",
            "https://example.atlassian.net.evil.test",
            "https://you:secret@example.atlassian.net",
            "https://example.atlassian.net/path",
        ] {
            assert!(cloud_site(bad).is_err());
        }
        assert!(
            env(&[("JIRA_API_TOKEN", "synthetic-token")])
                .credentials()
                .is_err()
        );
    }
    #[test]
    fn invalid_values_are_redacted_and_numbers_are_bounded() {
        let e = env(&[("TEST_NUMBER", "TOP_SECRET"), ("TEST_BOOL", "TOP_SECRET")]);
        assert!(
            !e.number("TEST_NUMBER", 2000)
                .unwrap_err()
                .to_string()
                .contains("TOP_SECRET")
        );
        assert!(
            !e.boolean("TEST_BOOL")
                .unwrap_err()
                .to_string()
                .contains("TOP_SECRET")
        );
        assert!(
            env(&[("TEST_NUMBER", "0")])
                .number("TEST_NUMBER", 2000)
                .is_err()
        );
        assert_eq!(
            env(&[("TEST_NUMBER", "17")])
                .number("TEST_NUMBER", 2000)
                .unwrap(),
            Some(17)
        );
    }
    #[test]
    fn private_file_loads_without_changing_process_environment() {
        use std::io::Write;
        let d = tempfile::tempdir().unwrap();
        // A private file in a normal public-readable project directory is valid.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(d.path(), fs::Permissions::from_mode(0o755)).unwrap();
        }
        let mut f = tempfile::NamedTempFile::new_in(d.path()).unwrap();
        writeln!(
            f,
            "TEST_JIRA_TUI_LOCAL_ONLY=synthetic\nTEST_QUOTED='a value'"
        )
        .unwrap();
        let e = Environment::load(Some(f.path())).unwrap();
        assert_eq!(e.value("TEST_QUOTED").as_deref(), Some("a value"));
        assert!(std::env::var("TEST_JIRA_TUI_LOCAL_ONLY").is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(f.path(), fs::Permissions::from_mode(0o644)).unwrap();
            assert!(Environment::load(Some(f.path())).is_err());
        }
    }
    #[test]
    fn malformed_file_does_not_include_secret_line_in_error() {
        use std::io::Write;
        let d = tempfile::tempdir().unwrap();
        let mut f = tempfile::NamedTempFile::new_in(d.path()).unwrap();
        writeln!(f, "TEST_SECRET='TOP_SECRET").unwrap();
        let error = Environment::load(Some(f.path())).err().unwrap().to_string();
        assert!(!error.contains("TOP_SECRET"));
    }
}
