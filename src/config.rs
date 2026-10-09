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
    pub backlog_statuses: Vec<String>,
    pub board_entry_status: String,
    pub backlog_entry_status: String,
    pub views: Vec<SavedView>,
    pub last_view: usize,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SavedView {
    pub name: String,
    pub jql: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            project: String::new(),
            prefix: String::new(),
            scope_jql: String::new(),
            scope_name: String::new(),
            backlog_statuses: Vec::new(),
            board_entry_status: String::new(),
            backlog_entry_status: String::new(),
            views: Vec::new(),
            last_view: 2,
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
        let mut out: Self = serde_json::from_value(value.clone())?;
        // Preserve a v0.1 user's project and saved queries without copying credentials.
        if out.project.is_empty() {
            out.project = value["filters"]["project"]
                .as_str()
                .unwrap_or("")
                .to_string()
        }
        if out.views.is_empty() {
            for p in value["saved"].as_array().into_iter().flatten() {
                out.views.push(SavedView {
                    name: p["name"].as_str().unwrap_or("").into(),
                    jql: p["jql"].as_str().unwrap_or("").into(),
                })
            }
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
        let (mut parts, order) = if view >= 4 {
            let preset = self.views.get(view - 4).context("Missing saved view")?;
            let (base, order) = split_order(&preset.jql);
            (vec![format!("({base})")], order)
        } else {
            (
                vec![
                    format!("({})", self.scope()?),
                    "statusCategory != Done".into(),
                ],
                "ORDER BY Rank ASC",
            )
        };
        if view <= 1 && self.backlog_statuses.is_empty() {
            bail!(
                "Choose backlog statuses in Scope (g) to define Board vs Backlog; use All meanwhile."
            )
        }
        match view {
            0 if !self.backlog_statuses.is_empty() => parts.push(format!(
                "status NOT IN ({})",
                self.backlog_statuses
                    .iter()
                    .map(|s| quote(s))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            1 => {
                if self.backlog_statuses.is_empty() {
                    bail!("Choose backlog statuses in Scope (g); no sprint assumptions are made.")
                };
                parts.push(format!(
                    "status IN ({})",
                    self.backlog_statuses
                        .iter()
                        .map(|s| quote(s))
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            }
            3 if assignee.trim().is_empty() => parts.push("assignee = currentUser()".into()),
            _ => {}
        }
        if !assignee.trim().is_empty() {
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
fn split_order(query: &str) -> (&str, &str) {
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
    fn scopes_and_backlog() {
        let s = Settings {
            project: "DEV".into(),
            backlog_statuses: vec!["Refinement".into()],
            ..Default::default()
        };
        let q = s.query(0, "", "", "").unwrap();
        assert!(q.contains("status NOT IN (\"Refinement\")"));
        assert!(!q.contains("sprint"));
        assert!(s.query(1, "", "", "").unwrap().contains("status IN"));
        assert_eq!(s.prefix(), "DEV");
        assert!(Settings::default().query(0, "", "", "").is_err());
    }
    #[test]
    fn saved_query_keeps_scope_and_applies_filters() {
        let s = Settings {
            views: vec![SavedView {
                name: "Other scope".into(),
                jql: "project = OTHER AND text ~ \"ORDER BY value\" ORDER BY updated DESC".into(),
            }],
            ..Default::default()
        };
        let q = s.query(4, "needle", "@me", "To Do").unwrap();
        assert!(q.contains("project = OTHER"));
        assert!(q.contains("text ~ \"needle\""));
        assert!(q.contains("assignee = currentUser()"));
        assert!(q.ends_with("ORDER BY updated DESC"));
        assert!(q.contains("\"ORDER BY value\""));
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
