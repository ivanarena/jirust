use crate::{
    data::{self, Board, Ticket},
    markdown,
};
use anyhow::{Context, Result, bail};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap, io::Write, path::PathBuf, process::Stdio, sync::Arc, time::Duration,
};
use tokio::{
    process::Command,
    sync::{Mutex, Semaphore},
    time::timeout,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Acli,
    Jira,
    Demo,
}
impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Acli => "acli",
            Self::Jira => "jira",
            Self::Demo => "demo",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Action {
    pub kind: String,
    pub key: String,
    pub value: String,
    pub extra: String,
}
impl Action {
    pub fn new(kind: &str, key: &str, value: &str) -> Self {
        Self {
            kind: kind.into(),
            key: key.into(),
            value: value.into(),
            extra: String::new(),
        }
    }
}
pub struct Backend {
    pub kind: Kind,
    pub binary: PathBuf,
    slots: Semaphore,
    demo: Mutex<BTreeMap<String, Ticket>>,
    #[cfg(test)]
    pub(crate) demo_moves: std::sync::atomic::AtomicUsize,
    api: std::sync::RwLock<Option<Arc<crate::board_api::BoardApi>>>,
    image_api: std::sync::RwLock<Option<Arc<crate::board_api::ImageApi>>>,
    image_slots: Semaphore,
    estimate_fields: tokio::sync::OnceCell<Vec<String>>,
    credentials: std::sync::RwLock<Option<Arc<crate::environment::Credentials>>>,
}
impl Backend {
    pub fn new(kind: &str, binary: Option<PathBuf>) -> Result<Arc<Self>> {
        let kind = match kind {
            "demo" => Kind::Demo,
            "acli" => Kind::Acli,
            "jira" => Kind::Jira,
            "auto" => {
                if find("acli").is_some() {
                    Kind::Acli
                } else {
                    Kind::Jira
                }
            }
            _ => bail!("Backend must be auto, acli, jira or demo"),
        };
        let binary = if kind == Kind::Demo {
            PathBuf::new()
        } else if let Some(p) = binary {
            p
        } else if kind == Kind::Jira && PathBuf::from("/usr/bin/jira").is_file() {
            PathBuf::from("/usr/bin/jira")
        } else {
            find(kind.name()).context("Install acli or jira-cli first")?
        };
        Ok(Arc::new(Self {
            kind,
            binary,
            #[cfg(test)]
            demo_moves: std::sync::atomic::AtomicUsize::new(0),
            slots: Semaphore::new(3),
            api: std::sync::RwLock::new(None),
            image_api: std::sync::RwLock::new(None),
            image_slots: Semaphore::new(2),
            estimate_fields: tokio::sync::OnceCell::new(),
            credentials: std::sync::RwLock::new(None),
            demo: Mutex::new(
                data::demo_tickets()
                    .into_iter()
                    .map(|t| (t.key.clone(), t))
                    .collect(),
            ),
        }))
    }
    pub fn configure_environment(
        &self,
        credentials: Option<Arc<crate::environment::Credentials>>,
    ) -> Result<()> {
        if let Some(c) = &credentials {
            if self.kind != Kind::Acli {
                bail!("Local API-token configuration requires the acli backend");
            }
            crate::board_api::check_environment(c)?;
        }
        *self.credentials.write().unwrap() = credentials;
        Ok(())
    }
    pub async fn authenticate(&self, credentials: &crate::environment::Credentials) -> Result<()> {
        use tokio::io::AsyncWriteExt;
        if self.kind != Kind::Acli {
            bail!("Environment authentication requires acli");
        }
        timeout(Duration::from_secs(45), async {
            let _permit = self.slots.acquire().await?;
            let mut child = Command::new(&self.binary)
                .args(["jira","auth","login","--site",&credentials.site,"--email",&credentials.email,"--token"])
                .env_remove("JIRA_API_TOKEN")
                .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).kill_on_drop(true).spawn()
                .context("Cannot start acli authentication")?;
            let mut stdin = child.stdin.take().context("Cannot open authentication input")?;
            stdin.write_all(credentials.token.as_bytes()).await.context("Cannot send authentication input")?;
            stdin.shutdown().await?; drop(stdin);
            let status = child.wait().await?;
            if !status.success() { bail!("acli authentication failed; check the local URL, email and token (values redacted)"); }
            Ok(())
        }).await.context("acli authentication timed out")?
    }
    pub fn configure_board(&self, id: Option<u64>) -> Result<()> {
        if self.kind == Kind::Demo {
            return Ok(());
        }
        let api = match id {
            Some(id) => {
                if self.kind != Kind::Acli {
                    bail!("Board API requires existing acli authentication")
                };
                Some(Arc::new(crate::board_api::BoardApi::with_credentials(
                    id,
                    self.credentials.read().unwrap().clone(),
                )?))
            }
            None => None,
        };
        *self.api.write().unwrap() = api;
        *self.image_api.write().unwrap() = if self.kind == Kind::Acli {
            crate::board_api::ImageApi::new(self.credentials.read().unwrap().clone())
                .ok()
                .map(Arc::new)
        } else {
            None
        };
        Ok(())
    }
    pub async fn image_bytes(
        &self,
        key: &str,
        attachment: &data::Attachment,
        cached: Option<Vec<u8>>,
    ) -> Result<Vec<u8>> {
        data::require_key(key)?;
        crate::images::validate_attachment(attachment)?;
        let _permit = self.image_slots.acquire().await?;
        let api=self.image_api.read().unwrap().clone().context("Automatic image downloads require an authenticated acli cloud profile; local images still work")?;
        api.check_identity()?;
        if let Some(bytes) = cached {
            return Ok(bytes);
        }
        // CLI remains the authority for the selected ticket's attachment list.
        let ticket = self.get(key).await?;
        if ticket.key != key
            || !ticket.attachments().iter().any(|a| {
                a.id == attachment.id && a.is_image() && a.size <= crate::images::MAX_BYTES
            })
        {
            bail!("Image attachment is not verified on the selected ticket");
        }
        let bytes = api.download(attachment).await?;
        api.check_identity()?;
        Ok(bytes)
    }
    async fn estimate_fields(&self) -> Vec<String> {
        self.estimate_fields
            .get_or_init(|| async {
                let api = self.image_api.read().unwrap().clone();
                if let Some(api) = api {
                    api.estimate_fields().await.unwrap_or_default()
                } else {
                    vec![]
                }
            })
            .await
            .clone()
    }
    pub fn board_identity(&self) -> Option<String> {
        self.api
            .read()
            .unwrap()
            .as_ref()
            .map(|a| a.cache_identity())
    }
    async fn run(&self, args: &[String]) -> Result<Vec<u8>> {
        if let Some(c) = self.credentials.read().unwrap().as_ref() {
            crate::board_api::check_environment(c)?;
        }
        let operation = async {
            let _permit = self.slots.acquire().await?;
            let mut command = Command::new(&self.binary);
            command
                .args(args)
                .env("NO_COLOR", "1")
                .env("CLICOLOR", "0")
                .stdin(Stdio::null())
                .kill_on_drop(true);
            if self.kind == Kind::Acli {
                command.env_remove("JIRA_API_TOKEN");
            }
            let (status, stdout) = crate::process::output(
                command,
                crate::limits::CLI_BYTES,
                crate::limits::STDERR_BYTES,
            )
            .await?;
            if !status.success() {
                bail!(
                    "{} failed (exit {:?}); diagnostic output suppressed to protect credentials. An update may have reached Jira; reload before retrying.",
                    self.kind.name(),
                    status.code()
                );
            }
            Ok(stdout)
        };
        timeout(Duration::from_secs(45), operation)
            .await
            .context("CLI timed out; an update may have reached Jira. Reload before retrying.")?
    }
    pub async fn scope(&self) -> String {
        if self.kind == Kind::Demo {
            return "demo".into();
        }
        if self.kind == Kind::Acli {
            return self
                .run(&strings(&["jira", "auth", "status"]))
                .await
                .map(|v| format!("{}:{}", self.binary.display(), String::from_utf8_lossy(&v)))
                .unwrap_or_default();
        }
        // Ask the CLI for identity/site; never read its credential files or tokens.
        let server_args = strings(&["serverinfo"]);
        let user_args = strings(&["me"]);
        let (server, user) = tokio::join!(self.run(&server_args), self.run(&user_args));
        match (server, user) {
            (Ok(server), Ok(user)) => format!(
                "{}:{}:{}",
                self.binary.display(),
                String::from_utf8_lossy(&server),
                String::from_utf8_lossy(&user)
            ),
            _ => String::new(), // Unknown identity: do not reuse a persistent cache.
        }
    }
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<Ticket>> {
        if self.kind == Kind::Demo {
            let mut tickets: Vec<Ticket> = self.demo.lock().await.values().cloned().collect();
            if query.contains("statusCategory != Done") {
                tickets.retain(|t| t.fields["status"]["statusCategory"]["key"] != "done")
            }
            let statuses =
                regex::Regex::new(r#"(?i)\bstatus\s+(NOT\s+)?IN\s*\(([^)]*)\)"#).unwrap();
            for capture in statuses.captures_iter(query) {
                if let Ok(values) =
                    serde_json::from_str::<Vec<String>>(&format!("[{}]", &capture[2]))
                {
                    let not = capture.get(1).is_some();
                    tickets.retain(|t| {
                        values.iter().any(|s| s.eq_ignore_ascii_case(&t.status())) != not
                    });
                }
            }
            for field in ["text", "status", "assignee", "project"] {
                let re =
                    regex::Regex::new(&format!(r#"(?i)\b{}\s*[=~]\s*(\"(?:\\.|[^\"])*\")"#, field))
                        .unwrap();
                if let Some(value) = re
                    .captures(query)
                    .and_then(|c| serde_json::from_str::<String>(&c[1]).ok())
                {
                    tickets.retain(|t| match field {
                        "text" => format!("{} {} {}", t.key, t.summary(), t.description())
                            .to_lowercase()
                            .contains(&value.trim_matches('*').to_lowercase()),
                        "status" => t.status().eq_ignore_ascii_case(&value),
                        "assignee" => {
                            let u = t.user("assignee");
                            [u.id, u.name, u.email]
                                .iter()
                                .any(|s| s.eq_ignore_ascii_case(&value))
                        }
                        _ => t.key.starts_with(&format!("{}-", value.to_uppercase())),
                    });
                }
            }
            if query.contains("assignee IS EMPTY") {
                tickets.retain(|t| t.fields["assignee"].is_null())
            }
            if query.contains("assignee = currentUser()") {
                tickets.retain(|t| t.user("assignee").id == "alex")
            }
            tickets.truncate(limit);
            return Ok(tickets);
        }
        let args = if self.kind == Kind::Acli {
            let mut a = strings(&[
                "jira",
                "workitem",
                "search",
                "--jql",
                query,
                "--fields",
                "key,summary,status,assignee,reporter,priority,issuetype,labels",
                "--limit",
            ]);
            a.push(limit.to_string());
            a.push("--json".into());
            a
        } else {
            strings(&[
                "issue",
                "list",
                "--jql",
                query,
                "--raw",
                "--paginate",
                &limit.min(100).to_string(),
            ])
        };
        collection(&self.run(&args).await?, &["issues", "values"])
    }
    async fn dataset(&self, query: &str, keys_only: bool) -> Result<Vec<Ticket>> {
        if self.kind != Kind::Acli {
            return self
                .search(
                    query,
                    if self.kind == Kind::Demo {
                        usize::MAX
                    } else {
                        100
                    },
                )
                .await;
        }
        // acli 1.3.x emits null records for a key-only projection. Summary
        // keeps its work-item mapper populated; membership still uses keys.
        let fields = if keys_only {
            "key,summary"
        } else {
            "key,summary,status,assignee,reporter,priority,issuetype,labels,description"
        };
        collection(
            &self
                .run(&strings(&[
                    "jira",
                    "workitem",
                    "search",
                    "--jql",
                    query,
                    "--fields",
                    fields,
                    "--paginate",
                    "--json",
                ]))
                .await?,
            &["issues", "values"],
        )
    }
    pub async fn workspace(
        &self,
        base: &str,
        mine: &str,
        changed: Option<&str>,
    ) -> Result<(crate::workspace::Workspace, Vec<String>)> {
        self.workspace_progress(base, mine, changed, |_| {}).await
    }
    pub async fn workspace_progress(
        &self,
        base: &str,
        mine: &str,
        changed: Option<&str>,
        report: impl Fn(crate::refresh::Stage) + Send + Sync,
    ) -> Result<(crate::workspace::Workspace, Vec<String>)> {
        use crate::refresh::Stage;
        let api = self.api.read().unwrap().clone();
        if let Some(api) = api {
            // Only OAuth needs acli to refresh its token. API-token requests
            // still validate the pinned profile/account/site on every operation.
            if api.uses_oauth() {
                report(Stage::Authentication);
                self.boards("").await?;
            }
            report(Stage::Fields);
            let fields = self.estimate_fields().await;
            report(Stage::Membership);
            let (mut workspace, mine, changes) = tokio::try_join!(
                api.workspace_progress(base, &fields, &report),
                self.dataset(mine, true),
                async {
                    match changed {
                        Some(q) => self.dataset(q, true).await,
                        None => Ok(Vec::new()),
                    }
                }
            )?;
            if !crate::workspace::valid_items(&mine) || !crate::workspace::valid_items(&changes) {
                bail!("CLI returned invalid keys")
            }
            report(Stage::Indexing);
            workspace.mine = mine.into_iter().map(|t| t.key).collect();
            workspace.encoded_bytes =
                crate::limits::size(&workspace, crate::limits::WORKSPACE_BYTES)?;
            return Ok((workspace, changes.into_iter().map(|t| t.key).collect()));
        }
        report(Stage::Tickets);
        let (items, mine, changes) =
            tokio::try_join!(self.dataset(base, false), self.dataset(mine, true), async {
                match changed {
                    Some(q) => self.dataset(q, true).await,
                    None => Ok(Vec::new()),
                }
            })?;
        if !crate::workspace::valid_items(&items)
            || !crate::workspace::valid_items(&mine)
            || !crate::workspace::valid_items(&changes)
        {
            bail!("CLI returned invalid ticket keys")
        }
        crate::limits::tickets(&items)?;
        let complete = self.kind != Kind::Jira
            || (items.len() < 100 && mine.len() < 100 && changes.len() < 100);
        // Keep the CLI's Rank sequence; removing children must
        // be a stable filter, never a key/date/status sort.
        let mut items: Vec<_> = items.into_iter().filter(Ticket::backlog_ticket).collect();
        let board = if self.kind == Kind::Demo {
            let keys: std::collections::HashSet<String> = items
                .iter()
                .filter(|t| {
                    t.fields["_jirust_board"]
                        .as_bool()
                        .unwrap_or(matches!(t.key.as_str(), "DEV-101" | "DEV-103"))
                })
                .map(|t| t.key.clone())
                .collect();
            let mut ordered: Vec<_> = items
                .iter()
                .filter(|t| keys.contains(&t.key))
                .cloned()
                .collect();
            ordered.extend(items.into_iter().filter(|t| !keys.contains(&t.key)));
            items = ordered;
            Some(keys)
        } else {
            None
        };
        let mut workspace = crate::workspace::Workspace {
            items,
            mine: mine.into_iter().map(|t| t.key).collect(),
            board,
            complete,
            synced_at: crate::config::now(),
            ..Default::default()
        };
        report(Stage::Indexing);
        crate::limits::size(&workspace, crate::limits::WORKSPACE_BYTES)?;
        workspace.reindex();
        Ok((workspace, changes.into_iter().map(|t| t.key).collect()))
    }
    pub async fn get(&self, key: &str) -> Result<Ticket> {
        data::require_key(key)?;
        if self.kind == Kind::Demo {
            return self
                .demo
                .lock()
                .await
                .get(key)
                .cloned()
                .context("Demo ticket not found");
        }
        let args = if self.kind == Kind::Acli {
            strings(&[
                "jira", "workitem", "view", key, "--fields", "*all", "--json",
            ])
        } else {
            strings(&["issue", "view", key, "--raw"])
        };
        let mut ticket: Ticket = serde_json::from_slice(&self.run(&args).await?).map_err(|_| {
            anyhow::anyhow!("CLI returned an unsupported ticket JSON shape (content suppressed)")
        })?;
        ticket.estimate_fields = self.estimate_fields().await;
        Ok(ticket)
    }
    pub async fn comments(&self, key: &str) -> Result<Vec<Value>> {
        data::require_key(key)?;
        if self.kind != Kind::Acli {
            let t = self.get(key).await?;
            let comments = t.fields["comment"]["comments"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            if t.fields["comment"]["total"].as_u64().unwrap_or(0) > comments.len() as u64 {
                bail!("Full comment pagination requires acli")
            };
            return Ok(comments);
        }
        collection(
            &self
                .run(&strings(&[
                    "jira",
                    "workitem",
                    "comment",
                    "list",
                    "--key",
                    key,
                    "--paginate",
                    "--json",
                ]))
                .await?,
            &["comments", "values"],
        )
    }
    pub async fn boards(&self, project: &str) -> Result<Vec<Board>> {
        if self.kind == Kind::Demo {
            return Ok(vec![Board {
                id: 1,
                name: "Demo board".into(),
                kind: "kanban".into(),
            }]);
        }
        if self.kind != Kind::Acli {
            bail!("Board discovery requires acli")
        };
        let mut args = strings(&["jira", "board", "search", "--json", "--limit", "100"]);
        if !project.is_empty() {
            args.extend(strings(&["--project", project]))
        }
        let values: Vec<Value> = collection(&self.run(&args).await?, &["values", "boards"])?;
        Ok(values
            .into_iter()
            .map(|v| Board {
                id: v["id"].as_u64().unwrap_or(0),
                name: data::text(&v["name"]),
                kind: data::text(&v["type"]),
            })
            .collect())
    }
    pub async fn apply(&self, a: &Action) -> Result<()> {
        data::require_key(&a.key)?;
        if let Some(api) = self.api.read().unwrap().as_ref() {
            api.check_identity()?;
        }
        if matches!(a.kind.as_str(), "board" | "backlog") && self.kind != Kind::Demo {
            let api = self
                .api
                .read()
                .unwrap()
                .clone()
                .context("Configure a Board API ID before moving tickets")?;
            if a.value != api.id.to_string() {
                bail!("Board changed since confirmation; move rejected")
            }
            return api.move_ticket(&a.key, a.kind == "board").await;
        }
        if a.value.trim().is_empty() {
            bail!("Empty update rejected")
        }
        if a.kind == "link" {
            data::require_key(&a.value)?;
            if a.extra.trim().is_empty() {
                bail!("Link type required")
            }
        }
        if self.kind == Kind::Demo {
            let mut map = self.demo.lock().await;
            let t = map.get_mut(&a.key).context("Demo ticket not found")?;
            match a.kind.as_str() {
                "board" | "backlog" => {
                    #[cfg(test)]
                    self.demo_moves
                        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    t.fields["_jirust_board"] = json!(a.kind == "board");
                }
                "status" => {
                    t.fields["status"]["name"] = json!(a.value);
                    t.fields["status"]["statusCategory"]["key"] =
                        json!(if a.value.eq_ignore_ascii_case("Done") {
                            "done"
                        } else {
                            "new"
                        });
                }
                "assign" => {
                    t.fields["assignee"] = if a.value == "unassigned" {
                        Value::Null
                    } else if a.value == "@me" {
                        json!({"displayName":"Alex","accountId":"alex","emailAddress":"alex@example.com"})
                    } else {
                        json!({"displayName":a.value,"accountId":a.value})
                    }
                }
                "summary" => t.fields["summary"] = json!(a.value),
                "description" => t.fields["description"] = markdown::to_adf(&a.value),
                "labels" => {
                    t.fields["labels"] =
                        json!(a.value.split(',').map(str::trim).collect::<Vec<_>>())
                }
                "comment" => {
                    let c = json!({"author":{"displayName":"You (demo)"},"body":markdown::to_adf(&a.value)});
                    if !t.fields["comment"]["comments"].is_array() {
                        t.fields["comment"] = json!({"comments":[]})
                    };
                    t.fields["comment"]["comments"]
                        .as_array_mut()
                        .unwrap()
                        .push(c)
                }
                _ => bail!("Action not simulated in demo"),
            };
            return Ok(());
        }
        let mut action = a.clone();
        if self.kind == Kind::Jira && action.kind == "assign" && action.value == "@me" {
            action.value = String::from_utf8(self.run(&strings(&["me"])).await?)?
                .trim()
                .into()
        }
        let mut body = None;
        if matches!(action.kind.as_str(), "description" | "comment") && self.kind == Kind::Acli {
            let mut file = tempfile::NamedTempFile::new()?;
            file.write_all(&serde_json::to_vec(&markdown::to_adf(&action.value))?)?;
            body = Some(file);
        }
        if action.kind == "comment" && self.kind == Kind::Jira {
            let mut f = tempfile::NamedTempFile::new()?;
            f.write_all(action.value.as_bytes())?;
            body = Some(f)
        }
        let args = self.action_args(
            &action,
            body.as_ref()
                .map(|f| f.path().to_string_lossy().to_string())
                .as_deref(),
        )?;
        receipt(&self.run(&args).await?)
    }
    pub fn action_args(&self, a: &Action, file: Option<&str>) -> Result<Vec<String>> {
        data::require_key(&a.key)?;
        if self.kind == Kind::Jira {
            return Ok(match a.kind.as_str() {
                "assign" => strings(&[
                    "issue",
                    "assign",
                    &a.key,
                    if a.value == "unassigned" {
                        "x"
                    } else {
                        &a.value
                    },
                ]),
                "status" => strings(&["issue", "move", &a.key, &a.value]),
                "summary" | "description" | "labels" => strings(&[
                    "issue",
                    "edit",
                    &a.key,
                    match a.kind.as_str() {
                        "summary" => "--summary",
                        "description" => "--body",
                        _ => "--label",
                    },
                    &a.value,
                    "--no-input",
                ]),
                "comment" => strings(&[
                    "issue",
                    "comment",
                    "add",
                    &a.key,
                    "--template",
                    file.context("Missing body file")?,
                    "--no-input",
                ]),
                "link" => {
                    data::require_key(&a.value)?;
                    strings(&["issue", "link", &a.key, &a.value, &a.extra])
                }
                _ => bail!("Unsupported jira-cli action"),
            });
        }
        Ok(match a.kind.as_str() {
            "assign" => {
                let mut args = strings(&[
                    "jira", "workitem", "assign", "--key", &a.key, "--yes", "--json",
                ]);
                if a.value == "unassigned" {
                    args.push("--remove-assignee".into())
                } else {
                    args.extend(strings(&["--assignee", &a.value]))
                };
                args
            }
            "status" => strings(&[
                "jira",
                "workitem",
                "transition",
                "--key",
                &a.key,
                "--status",
                &a.value,
                "--yes",
                "--json",
            ]),
            "summary" | "labels" | "type" => strings(&[
                "jira",
                "workitem",
                "edit",
                "--key",
                &a.key,
                &format!("--{}", a.kind),
                &a.value,
                "--yes",
                "--json",
            ]),
            "description" => strings(&[
                "jira",
                "workitem",
                "edit",
                "--key",
                &a.key,
                "--description-file",
                file.context("Missing body file")?,
                "--yes",
                "--json",
            ]),
            "comment" => strings(&[
                "jira",
                "workitem",
                "comment",
                "create",
                "--key",
                &a.key,
                "--body-file",
                file.context("Missing body file")?,
                "--json",
            ]),
            "link" => {
                data::require_key(&a.value)?;
                strings(&[
                    "jira", "workitem", "link", "create", "--out", &a.key, "--in", &a.value,
                    "--type", &a.extra, "--yes",
                ])
            }
            _ => bail!("Unsupported acli action"),
        })
    }
}
fn find(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|p| p.join(name))
        .find(|p| p.is_file())
}
fn strings(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}
fn collection<T: DeserializeOwned>(bytes: &[u8], keys: &[&str]) -> Result<Vec<T>> {
    let mut v: Value = serde_json::from_slice(bytes)
        .map_err(|_| anyhow::anyhow!("CLI did not return valid JSON (content suppressed)"))?;
    let array = if v.is_array() {
        v
    } else {
        let key = keys.iter().find(|key| v[**key].is_array()).ok_or_else(|| {
            anyhow::anyhow!("CLI collection shape unsupported; no result silently discarded")
        })?;
        v[*key].take()
    };
    if array
        .as_array()
        .is_some_and(|a| a.len() > crate::limits::RECORDS)
    {
        bail!("CLI collection exceeds record limit; narrow the scope");
    }
    serde_json::from_value(array)
        .map_err(|_| anyhow::anyhow!("CLI collection shape unsupported (content suppressed)"))
}
fn receipt(bytes: &[u8]) -> Result<()> {
    fn failed(v: &Value) -> bool {
        match v {
            Value::Array(a) => a.iter().any(failed),
            Value::Object(m) => m.iter().any(|(k, v)| {
                let k = k.to_lowercase();
                (k == "success" && v.as_bool() == Some(false))
                    || (matches!(
                        k.as_str(),
                        "errors" | "errormessages" | "failures" | "failed"
                    ) && match v {
                        Value::Array(a) => !a.is_empty(),
                        Value::Object(m) => !m.is_empty(),
                        Value::String(s) => !s.is_empty(),
                        Value::Bool(b) => *b,
                        Value::Number(n) => n.as_u64().unwrap_or(0) > 0,
                        _ => false,
                    })
                    || failed(v)
            }),
            _ => false,
        }
    }
    if let Ok(v) = serde_json::from_slice::<Value>(bytes)
        && failed(&v)
    {
        bail!(
            "CLI reported unsuccessful update (content suppressed); outcome may be unknown. Reload before retrying."
        )
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[tokio::test]
    async fn authentication_uses_stdin_not_arguments_and_redacts_cli_output() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let cli = dir.path().join("fake-acli");
        let c = crate::environment::Credentials {
            site: "example.atlassian.net".into(),
            email: "alex@example.com".into(),
            token: "synthetic-secret".into(),
        };
        std::fs::write(&cli,"#!/bin/sh\n[ \"$#\" = 8 ] || exit 1\nfor arg in \"$@\"; do [ \"$arg\" != synthetic-secret ] || exit 1; done\ninput=$(cat)\n[ \"$input\" = synthetic-secret ] || exit 1\nexit 0\n").unwrap();
        std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o700)).unwrap();
        let b = Backend::new("acli", Some(cli.clone())).unwrap();
        b.authenticate(&c).await.unwrap();
        std::fs::write(
            &cli,
            "#!/bin/sh\ninput=$(cat)\nprintf '%s' \"$input\" >&2\nexit 1\n",
        )
        .unwrap();
        assert!(
            !b.authenticate(&c)
                .await
                .unwrap_err()
                .to_string()
                .contains("synthetic-secret")
        );
    }
    #[test]
    fn cli_action_arguments() {
        let b = Backend::new("acli", Some("/fake/acli".into())).unwrap();
        let a = Action::new("status", "DEV-1", "In Progress");
        assert_eq!(
            b.action_args(&a, None).unwrap(),
            strings(&[
                "jira",
                "workitem",
                "transition",
                "--key",
                "DEV-1",
                "--status",
                "In Progress",
                "--yes",
                "--json"
            ])
        );
        let a = Action::new("assign", "DEV-1", "unassigned");
        assert!(
            b.action_args(&a, None)
                .unwrap()
                .contains(&"--remove-assignee".into())
        );
        assert!(
            b.action_args(&Action::new("summary", "--help", "x"), None)
                .is_err()
        );
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn ordinary_errors_and_success_exit_failure_receipts_never_expose_output() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let cli = d.path().join("fake-acli");
        std::fs::write(&cli, "#!/bin/sh\nprintf 'Authorization: Bearer SYNTHETIC_ONLY; https://user:password@example.com' >&2\nexit 7\n").unwrap();
        std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o700)).unwrap();
        let b = Backend::new("acli", Some(cli)).unwrap();
        let error = format!("{:#}", b.run(&[]).await.unwrap_err());
        assert!(error.contains("exit Some(7)"));
        for secret in ["SYNTHETIC_ONLY", "password", "Authorization"] {
            assert!(!error.contains(secret));
        }
        let error = format!(
            "{:#}",
            receipt(br#"{"errors":["SYNTHETIC_ONLY"]}"#).unwrap_err()
        );
        assert!(!error.contains("SYNTHETIC_ONLY"));
        let error = format!(
            "{:#}",
            collection::<Ticket>(br#"[{"key":"DEV-1","transitions":"SYNTHETIC_ONLY"}]"#, &[])
                .unwrap_err()
        );
        assert!(!error.contains("SYNTHETIC_ONLY"));
    }
    #[test]
    fn response_shapes_and_receipts() {
        for s in ["[]", "{\"issues\":[]}", "{\"values\":[]}"] {
            assert!(collection::<Ticket>(s.as_bytes(), &["issues", "values"]).is_ok())
        }
        assert!(receipt(b"{\"success\":false}").is_err());
        assert!(receipt(b"{\"errors\":[\"denied\"]}").is_err());
        assert!(receipt(b"{\"success\":true}").is_ok());
    }
    #[tokio::test]
    async fn assignment_views_include_both_board_and_backlog_statuses() {
        let b = Backend::new("demo", None).unwrap();
        {
            let mut tickets = b.demo.lock().await;
            let mut board_unassigned = tickets["DEV-102"].clone();
            board_unassigned.key = "DEV-104".into();
            board_unassigned.fields["status"]["name"] = json!("To Do");
            tickets.insert(board_unassigned.key.clone(), board_unassigned);
            let mut backlog_mine = tickets["DEV-101"].clone();
            backlog_mine.key = "DEV-105".into();
            backlog_mine.fields["status"]["name"] = json!("Refinement");
            tickets.insert(backlog_mine.key.clone(), backlog_mine);
        }
        let s = crate::config::Settings {
            project: "DEV".into(),
            ..Default::default()
        };
        let mine = b
            .search(&s.query(crate::config::MINE, "", "", "").unwrap(), 500)
            .await
            .unwrap();
        let available = b
            .search(&s.query(crate::config::AVAILABLE, "", "", "").unwrap(), 500)
            .await
            .unwrap();
        let backlog = b
            .search(&s.query(crate::config::BACKLOG, "", "", "").unwrap(), 500)
            .await
            .unwrap();
        assert_eq!(
            mine.iter().map(|t| t.key.as_str()).collect::<Vec<_>>(),
            ["DEV-101", "DEV-105"]
        );
        assert_eq!(
            available.iter().map(|t| t.key.as_str()).collect::<Vec<_>>(),
            ["DEV-102", "DEV-104"]
        );
        assert_eq!(backlog.len(), 5);
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn membership_projection_avoids_acli_key_only_null_records() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let cli = dir.path().join("fake-acli");
        std::fs::write(&cli,"#!/bin/sh\nwhile [ $# -gt 0 ]; do\n if [ \"$1\" = --fields ] && [ \"$2\" = key ]; then printf '[null]'; exit 0; fi\n shift\ndone\nprintf '[{\"key\":\"DEV-1\",\"fields\":{\"summary\":\"Synthetic\"}}]'\n").unwrap();
        std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o700)).unwrap();
        let b = Backend::new("acli", Some(cli)).unwrap();
        let items = b.dataset("project = DEV", true).await.unwrap();
        assert_eq!(items[0].key, "DEV-1");
    }
    #[tokio::test]
    async fn demo_search_really_filters() {
        let b = Backend::new("demo", None).unwrap();
        let q = "project = \"DEV\" AND text ~ \"assignment\" ORDER BY Rank ASC";
        assert_eq!(b.search(q, 500).await.unwrap().len(), 1);
        let q = "project = \"DEV\" AND assignee IS EMPTY";
        assert_eq!(b.search(q, 500).await.unwrap()[0].key, "DEV-102");
    }
    #[tokio::test]
    async fn demo_membership_moves_do_not_transition_status() {
        let b = Backend::new("demo", None).unwrap();
        let settings = crate::config::Settings {
            project: "DEV".into(),
            board_id: Some(1),
            ..Default::default()
        };
        let before = b.get("DEV-102").await.unwrap().status();
        b.apply(&Action::new("board", "DEV-102", "1"))
            .await
            .unwrap();
        let q = settings.query(crate::config::BACKLOG, "", "", "").unwrap();
        let mine = settings.query(crate::config::MINE, "", "", "").unwrap();
        assert!(
            b.workspace(&q, &mine, None)
                .await
                .unwrap()
                .0
                .board
                .unwrap()
                .contains("DEV-102")
        );
        assert_eq!(b.get("DEV-102").await.unwrap().status(), before);
        b.apply(&Action::new("backlog", "DEV-102", "1"))
            .await
            .unwrap();
        assert!(
            !b.workspace(&q, &mine, None)
                .await
                .unwrap()
                .0
                .board
                .unwrap()
                .contains("DEV-102")
        );
    }
    #[tokio::test]
    async fn fake_mutations_only() {
        let b = Backend::new("demo", None).unwrap();
        b.apply(&Action::new("assign", "DEV-101", "unassigned"))
            .await
            .unwrap();
        assert_eq!(
            b.get("DEV-101").await.unwrap().user("assignee").label(),
            "Unassigned"
        );
    }
}
