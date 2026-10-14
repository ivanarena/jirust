//! Browser/clipboard-only URLs. Never an authenticated API endpoint or origin.
use anyhow::{Result, ensure};
use url::Url;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowseBase(Url);
impl BrowseBase {
    pub fn parse(value: &str) -> Result<Self> {
        let error = "Invalid browser base URL (value redacted); use HTTP(S) host with an optional plain context path, no credentials/query/fragment or /browse suffix";
        ensure!(
            value.len() <= 4096
                && !value.chars().any(char::is_whitespace)
                && !value.contains(['%', '\\']),
            "{error}"
        );
        let mut url = Url::parse(value).map_err(|_| anyhow::anyhow!(error))?;
        ensure!(
            crate::data::valid_url(value)
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "{error}"
        );
        ensure!(
            url.path()
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"/-_.~".contains(&b))
                && !url.path().contains("//")
                && !url
                    .path()
                    .split('/')
                    .any(|s| matches!(s, "." | ".." | "browse")),
            "{error}"
        );
        let path = url.path().trim_end_matches('/').to_owned();
        url.set_path(&format!("{path}/"));
        Ok(Self(url))
    }
    pub fn ticket(&self, key: &str) -> Option<String> {
        crate::data::valid_key(key)
            .then(|| self.0.join(&format!("browse/{key}")).ok())
            .flatten()
            .map(|u| u.to_string())
    }
    pub fn from_issue_self(value: &str) -> Option<Self> {
        let mut u = Url::parse(value).ok()?;
        // OAuth's API gateway is not a browser site. Require the pinned profile site
        // or an explicit UI-only base rather than generating a misleading link.
        if u.host_str() == Some("api.atlassian.com") {
            return None;
        }
        if !crate::data::valid_url(value) || !u.username().is_empty() || u.password().is_some() {
            return None;
        }
        let context = u
            .path()
            .split_once("/rest/")
            .map(|(p, _)| p)
            .unwrap_or("")
            .to_owned();
        u.set_path(&format!("{context}/"));
        u.set_query(None);
        u.set_fragment(None);
        Self::parse(u.as_str()).ok()
    }
    pub fn issue_key(&self, url: &str) -> Option<String> {
        let u = Url::parse(url).ok()?;
        if u.origin() != self.0.origin() || !u.username().is_empty() || u.password().is_some() {
            return None;
        }
        let prefix = format!("{}browse/", self.0.path());
        let key = u.path().strip_prefix(&prefix)?.trim_end_matches('/');
        crate::data::valid_key(key).then(|| key.to_owned())
    }
}
/// Clipboard paths may include a configured context, but never arbitrary suffixes.
pub fn browse_key(path: &str) -> Option<&str> {
    let (context, key) = path.rsplit_once("/browse/")?;
    if context
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"/-_.~".contains(&b))
        && !context.contains("//")
        && !context.split('/').any(|s| matches!(s, "." | ".."))
        && crate::data::valid_key(key)
    {
        Some(key)
    } else {
        None
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn context_links_are_ui_only_and_reject_credentials_or_unsafe_paths_without_echoing_them() {
        let base = BrowseBase::parse("https://jira.example.org:8443/jira/team/").unwrap();
        let link = base.ticket("DEV-123").unwrap();
        assert_eq!(
            link,
            "https://jira.example.org:8443/jira/team/browse/DEV-123"
        );
        assert_eq!(base.issue_key(&link).as_deref(), Some("DEV-123"));
        crate::clipboard::validate(&link).unwrap();
        assert!(base.ticket("DEV-1\x1b").is_none());
        for bad in [
            "https://alex:synthetic-secret@example.org",
            "https://example.org/?token=synthetic-secret",
            "https://example.org/#synthetic-secret",
            "file:///tmp/secret",
            "https://example.org/browse",
            "https://example.org/a%2fb",
            "https://example.org/a/%2e/b",
            "https://example.org/a\\\\b",
            "https://example.org/a//b",
        ] {
            let error = BrowseBase::parse(bad).unwrap_err().to_string();
            assert!(!error.contains("synthetic-secret"));
        }
    }
    #[test]
    fn inferred_links_preserve_server_context_and_never_link_to_the_oauth_gateway() {
        assert_eq!(
            BrowseBase::from_issue_self("https://jira.example.org/jira/rest/api/2/issue/1")
                .unwrap()
                .ticket("DEV-1")
                .unwrap(),
            "https://jira.example.org/jira/browse/DEV-1"
        );
        assert!(
            BrowseBase::from_issue_self(
                "https://api.atlassian.com/ex/jira/cloud/rest/api/3/issue/1"
            )
            .is_none()
        );
        assert!(
            BrowseBase::from_issue_self("https://alex:secret@example.org/rest/api/2/issue/1")
                .is_none()
        );
    }
}
