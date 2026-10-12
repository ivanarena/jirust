//! Explicit local configuration; never put credentials in Settings or Debug output.
use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeMap,
    fs,
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
impl Environment {
    pub fn load(explicit: Option<&Path>) -> Result<Self> {
        let path = match explicit {
            Some(path) => Some(path.to_owned()),
            None if Path::new(".env").exists() => Some(PathBuf::from(".env")),
            None => Some(crate::config::config_directory()?.join(".env")).filter(|p| p.exists()),
        };
        let Some(path) = path else {
            return Ok(Self::default());
        };
        let metadata = fs::symlink_metadata(&path).context("Cannot read local environment file")?;
        ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "Environment file must be a regular file, not a symlink"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            ensure!(
                metadata.permissions().mode() & 0o077 == 0,
                "Environment file must be private; run chmod 600 on it"
            );
        }
        ensure!(metadata.len() <= 65536, "Environment file exceeds 64 KiB");
        let bytes = fs::read(&path).context("Cannot read local environment file")?;
        ensure!(bytes.len() <= 65536, "Environment file exceeds 64 KiB");
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
    fn env(values: &[(&str, &str)]) -> Environment {
        Environment {
            values: values
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
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
        let mut f = tempfile::NamedTempFile::new().unwrap();
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
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "TEST_SECRET='TOP_SECRET").unwrap();
        let error = Environment::load(Some(f.path())).err().unwrap().to_string();
        assert!(!error.contains("TOP_SECRET"));
    }
}
