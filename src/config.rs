use anyhow::{Context, Result, bail};
use directories::BaseDirs;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub project: String,
    pub prefix: String,
    pub scope_jql: String,
    pub scope_name: String,
    pub last_view: usize,
    pub view_layout: u8,
}
pub const MINE: usize = 0;
pub const AVAILABLE: usize = 1;
pub const BACKLOG: usize = 2;
pub const VIEW_NAMES: [&str; 3] = ["Mine", "Available", "Backlog"];
impl Default for Settings {
    fn default() -> Self {
        Self {
            project: String::new(),
            prefix: String::new(),
            scope_jql: String::new(),
            scope_name: String::new(),
            last_view: MINE,
            view_layout: 3,
        }
    }
}
impl Settings {
    pub fn prefix(&self) -> &str {
        if self.prefix.is_empty() {
            &self.project
        } else {
            &self.prefix
        }
    }
    pub fn load() -> Result<Self> {
        let path = config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let value: Value = serde_json::from_slice(&fs::read(path)?)?;
        Self::from_value(value)
    }
    fn from_value(value: Value) -> Result<Self> {
        let mut out: Self = serde_json::from_value(value.clone())?;
        // Migrate old numeric tabs without reinterpreting Board as Mine or
        // old Backlog-only as Available. All three now share one backlog scope.
        if value["view_layout"].as_u64() != Some(3) {
            out.last_view = match value["last_view"].as_u64() {
                Some(3) => MINE,
                Some(_) => BACKLOG,
                None => MINE,
            };
        }
        out.view_layout = 3;
        out.last_view = out.last_view.min(BACKLOG);
        // Preserve the configured project/scope, never Jira credentials.
        if out.project.is_empty() {
            out.project = value["filters"]["project"]
                .as_str()
                .unwrap_or("")
                .to_string()
        }
        Ok(out)
    }
    pub fn save(&self) -> Result<()> {
        private_write(&config_path()?, &serde_json::to_vec_pretty(self)?)
    }
    pub fn scope(&self) -> Result<String> {
        if !self.scope_jql.trim().is_empty() {
            return Ok(self.scope_jql.trim().into());
        };
        if self.project.trim().is_empty() {
            bail!("Choose a project/scope first (g).")
        };
        Ok(format!("project = {}", quote(&self.project)))
    }
    pub fn query(&self, view: usize, search: &str, assignee: &str, status: &str) -> Result<String> {
        let scope = self.scope()?;
        let (base, order) = split_order(&scope);
        let mut parts = vec![format!("({base})"), "statusCategory != Done".into()];
        match view {
            MINE => parts.push("assignee = currentUser()".into()),
            AVAILABLE => parts.push("assignee IS EMPTY".into()),
            BACKLOG => {}
            _ => bail!("Unknown view"),
        }
        // Manual assignee filtering belongs to the combined Backlog view;
        // it cannot silently turn Mine or Available into a different preset.
        if view == BACKLOG && !assignee.trim().is_empty() {
            parts.push(match assignee.trim() {
                "@me" => "assignee = currentUser()".into(),
                "unassigned" => "assignee IS EMPTY".into(),
                value => format!("assignee = {}", quote(value)),
            })
        }
        if !status.trim().is_empty() {
            parts.push(format!("status = {}", quote(status)))
        }
        if !search.trim().is_empty() {
            parts.push(format!("text ~ {}", quote(search.trim())))
        }
        Ok(format!("{} {order}", parts.join(" AND ")))
    }
}
pub(crate) fn split_order(query: &str) -> (&str, &str) {
    let mut quote = None;
    let mut escaped = false;
    for (n, c) in query.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' {
            escaped = true;
            continue;
        }
        if let Some(q) = quote {
            if c == q {
                quote = None
            };
            continue;
        }
        if matches!(c, '\'' | '"') {
            quote = Some(c);
            continue;
        }
        if (n == 0 || query[..n].ends_with(char::is_whitespace))
            && query[n..]
                .get(..9)
                .is_some_and(|s| s.eq_ignore_ascii_case("ORDER BY "))
        {
            return (query[..n].trim(), query[n..].trim());
        }
    }
    (query.trim(), "ORDER BY Rank ASC")
}
pub fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}
pub fn attachment_directory(scope: &str) -> Result<PathBuf> {
    Ok(BaseDirs::new()
        .context("No user directory")?
        .cache_dir()
        .join("jiraui/rust")
        .join(hash(scope))
        .join("attachments"))
}
pub fn config_path() -> Result<PathBuf> {
    Ok(BaseDirs::new()
        .context("No user directory")?
        .config_dir()
        .join("jiraui/config.json"))
}
fn private_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
pub fn private_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("Missing parent directory")?;
    private_dir(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(bytes)?;
    file.persist(path)?;
    Ok(())
}
fn hash(s: &str) -> String {
    format!("{:x}", Sha256::digest(s.as_bytes()))
}
#[derive(Clone)]
pub struct Cache {
    pub directory: Option<PathBuf>,
}
#[derive(Serialize, Deserialize)]
struct Entry {
    time: u64,
    data: Value,
}
impl Cache {
    pub fn new(scope: &str, disabled: bool) -> Result<Self> {
        if disabled || scope.is_empty() {
            return Ok(Self { directory: None });
        }
        let path = BaseDirs::new()
            .context("No user directory")?
            .cache_dir()
            .join("jiraui/rust")
            .join(hash(scope));
        private_dir(&path)?;
        Ok(Self {
            directory: Some(path),
        })
    }
    pub fn get<T: serde::de::DeserializeOwned>(&self, key: &str) -> Option<(T, u64)> {
        let bytes = fs::read(self.directory.as_ref()?.join(format!("{}.json", hash(key)))).ok()?;
        let entry: Entry = serde_json::from_slice(&bytes).ok()?;
        Some((serde_json::from_value(entry.data).ok()?, entry.time))
    }
    pub fn put(&self, key: &str, data: &impl Serialize) -> Result<()> {
        let Some(path) = &self.directory else {
            return Ok(());
        };
        let e = Entry {
            time: now(),
            data: serde_json::to_value(data)?,
        };
        private_write(
            &path.join(format!("{}.json", hash(key))),
            &serde_json::to_vec(&e)?,
        )
    }
    pub fn exists(&self, key: &str) -> bool {
        self.directory
            .as_ref()
            .is_some_and(|dir| dir.join(format!("{}.json", hash(key))).is_file())
    }
    pub fn remove(&self, key: &str) {
        if let Some(dir) = &self.directory {
            let _ = fs::remove_file(dir.join(format!("{}.json", hash(key))));
        }
    }
}
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn local_attachment(root: &Path, key: &str, filename: &str) -> Option<PathBuf> {
    if !crate::data::valid_key(key) {
        return None;
    };
    let name = Path::new(filename).file_name()?;
    if name != filename || filename.contains('\\') {
        return None;
    };
    let root = root.canonicalize().ok()?;
    let candidate = root.join(key).join(name).canonicalize().ok()?;
    (candidate.starts_with(root) && candidate.is_file()).then_some(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn three_assignment_views_share_the_combined_backlog_scope() {
        let s = Settings {
            project: "DEV".into(),
            ..Default::default()
        };
        let base = s.query(BACKLOG, "", "", "").unwrap();
        assert!(!base.contains("assignee"));
        for view in [MINE, AVAILABLE, BACKLOG] {
            let q = s.query(view, "", "", "").unwrap();
            assert!(q.starts_with("(project = \"DEV\") AND statusCategory != Done"));
            assert!(!q.contains("sprint"));
            assert!(!q.contains("status IN"));
            assert!(!q.contains("status NOT IN"));
        }
        assert!(
            s.query(MINE, "", "", "")
                .unwrap()
                .contains("assignee = currentUser()")
        );
        assert!(
            s.query(AVAILABLE, "", "", "")
                .unwrap()
                .contains("assignee IS EMPTY")
        );
        assert!(!s.query(MINE, "", "sam", "").unwrap().contains("\"sam\""));
        assert_eq!(s.prefix(), "DEV");
    }
    #[test]
    fn base_scope_keeps_its_sort_and_additional_filters() {
        let s = Settings {
            scope_jql: "project = DEV AND text ~ \"ORDER BY value\" ORDER BY updated DESC".into(),
            ..Default::default()
        };
        let q = s.query(BACKLOG, "needle", "@me", "To Do").unwrap();
        assert!(q.contains("project = DEV"));
        assert!(q.contains("text ~ \"needle\""));
        assert!(q.contains("assignee = currentUser()"));
        assert!(q.ends_with("ORDER BY updated DESC"));
        assert!(q.contains("\"ORDER BY value\""));
    }
    #[test]
    fn migrate_old_tabs_and_remove_obsolete_setup() {
        for (old, new) in [
            (0, BACKLOG),
            (1, BACKLOG),
            (2, BACKLOG),
            (3, MINE),
            (8, BACKLOG),
        ] {
            let s=Settings::from_value(serde_json::json!({"project":"DEV","last_view":old,"backlog_statuses":[],"board_entry_status":"","views":[{"name":"old","jql":"project = OTHER"}]})).unwrap();
            assert_eq!(s.last_view, new);
            assert_eq!(s.view_layout, 3);
            let json = serde_json::to_value(&s).unwrap();
            assert!(json.get("backlog_statuses").is_none());
            assert!(json.get("views").is_none());
        }
        let s = Settings::from_value(
            serde_json::json!({"project":"DEV","view_layout":3,"last_view":AVAILABLE}),
        )
        .unwrap();
        assert_eq!(s.last_view, AVAILABLE);
    }
    #[test]
    fn query_escape() {
        assert_eq!(quote("x\" OR status=Done"), "\"x\\\" OR status=Done\"");
    }
    #[test]
    fn private_file_and_cache() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("prefs.json");
        private_write(&p, b"{}").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(p).unwrap().permissions().mode() & 0o777, 0o600);
        }
        let cache = Cache {
            directory: Some(d.path().to_path_buf()),
        };
        cache.put("private query", &vec!["DEV-1"]).unwrap();
        assert_eq!(
            cache.get::<Vec<String>>("private query").unwrap().0,
            vec!["DEV-1"]
        );
        cache.remove("private query");
        assert!(cache.get::<Value>("private query").is_none());
    }
    #[test]
    fn attachment_paths() {
        let d = tempfile::tempdir().unwrap();
        fs::create_dir(d.path().join("DEV-1")).unwrap();
        fs::write(d.path().join("DEV-1/picture.png"), b"fake").unwrap();
        assert!(local_attachment(d.path(), "DEV-1", "picture.png").is_some());
        assert!(local_attachment(d.path(), "DEV-1", "../picture.png").is_none());
    }
}
