//! Opt-in Jira Software API bridge. Existing acli authentication is reused;
//! OAuth login/refresh stays in acli. Never serialize or debug credentials.
use crate::{
    data::{self, Ticket},
    workspace::Workspace,
};
use anyhow::{Context, Result, bail, ensure};
use directories::BaseDirs;
use flate2::read::GzDecoder;
use reqwest::{Client, Method, Url};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{io::Read, process::Stdio, time::Duration};
use tokio::{process::Command, sync::Semaphore, time::timeout};

#[derive(Clone, Deserialize, PartialEq, Eq)]
struct Profile {
    site: String,
    cloud_id: String,
    account_id: String,
    email: String,
    auth_type: String,
}
#[derive(Deserialize)]
struct CliConfig {
    current_profile: String,
    profiles: Vec<Profile>,
}
impl Profile {
    fn is_oauth(&self) -> bool {
        matches!(self.auth_type.as_str(), "oauth" | "oauth_global")
    }
    fn keyring_username(&self) -> String {
        format!(
            "{}:{}:{}",
            if self.auth_type == "oauth_global" {
                "oauth"
            } else {
                "jira"
            },
            self.cloud_id,
            self.account_id
        )
    }
}
fn cli_profile(file: &str) -> Result<Profile> {
    let path = BaseDirs::new()
        .context("No home directory")?
        .config_dir()
        .join("acli")
        .join(file);
    let bytes = std::fs::read(path).context("Cannot read existing acli profile metadata")?;
    ensure!(bytes.len() < 1024 * 1024, "acli metadata is too large");
    let c: CliConfig = serde_yaml::from_slice(&bytes)
        .map_err(|_| anyhow::anyhow!("Invalid acli profile metadata"))?;
    c.profiles
        .into_iter()
        .find(|p| format!("{}:{}", p.cloud_id, p.account_id) == c.current_profile)
        .context("No current acli Jira profile")
}
fn profile() -> Result<Profile> {
    let p = cli_profile("jira_config.yaml")?;
    if p.auth_type == "oauth_global" {
        let global = cli_profile("global_auth_config.yaml")?;
        ensure!(
            global.cloud_id == p.cloud_id
                && global.account_id == p.account_id
                && global.site == p.site
                && global.email == p.email
                && global.auth_type == "oauth",
            "Global OAuth/Jira identities differ; select the same account/site in acli and restart jiraui"
        );
    }
    Ok(p)
}
pub struct BoardApi {
    pub id: u64,
    profile: Profile,
    client: Client,
    slots: Semaphore,
}
impl BoardApi {
    pub fn new(id: u64) -> Result<Self> {
        ensure!(id > 0, "Board ID must be positive");
        let p = profile()?;
        ensure!(
            p.cloud_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
                && !p.cloud_id.is_empty(),
            "Invalid cloud identifier"
        );
        let site = Url::parse(&format!("https://{}", p.site)).context("Invalid acli site")?;
        ensure!(
            site.host_str()
                .is_some_and(|h| h.ends_with(".atlassian.net"))
                && site.username().is_empty()
                && site.password().is_none()
                && site.path() == "/"
                && site.query().is_none()
                && site.fragment().is_none()
                && site.port().is_none(),
            "Expected an Atlassian Cloud site"
        );
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(45))
            .build()?;
        Ok(Self {
            id,
            profile: p,
            client,
            slots: Semaphore::new(2),
        })
    }
    pub fn check_identity(&self) -> Result<()> {
        ensure!(
            profile()? == self.profile,
            "Jira account/site/authentication changed; restart jiraui"
        );
        Ok(())
    }
    pub fn cache_identity(&self) -> String {
        format!(
            "board-api-v2:{}:{}:{}",
            self.profile.cloud_id, self.profile.account_id, self.id
        )
    }
    async fn credential(&self) -> Result<String> {
        self.check_identity()?;
        let username = self.profile.keyring_username();
        let out = timeout(
            Duration::from_secs(10),
            Command::new("secret-tool")
                .args(["lookup", "service", "acli", "username", &username])
                .stdin(Stdio::null())
                .kill_on_drop(true)
                .output(),
        )
        .await
        .context("Keyring lookup timed out")?
        .context("Install secret-tool to reuse the acli Linux keyring")?;
        ensure!(
            out.status.success() && !out.stdout.is_empty(),
            "Cannot access current acli keyring credential"
        );
        decode_credential(&out.stdout, self.profile.is_oauth())
    }
    fn url(&self, endpoint: &str) -> Result<Url> {
        let origin = if self.profile.is_oauth() {
            format!(
                "https://api.atlassian.com/ex/jira/{}",
                self.profile.cloud_id
            )
        } else {
            format!("https://{}", self.profile.site)
        };
        let path = if endpoint.starts_with("greenhopper/1.0/") {
            format!("/rest/{endpoint}")
        } else {
            format!("/rest/agile/1.0/{endpoint}")
        };
        Ok(Url::parse(&format!("{origin}{path}"))?)
    }
    async fn request(
        &self,
        method: Method,
        endpoint: &str,
        query: &[(&str, String)],
        body: Option<Value>,
    ) -> Result<Value> {
        let _permit = self.slots.acquire().await?;
        let token = self.credential().await?;
        let mut request = self
            .client
            .request(method.clone(), self.url(endpoint)?)
            .query(query)
            .header("Accept", "application/json");
        if method == Method::GET {
            request = request.header("Cache-Control", "no-cache");
        }
        request = if self.profile.is_oauth() {
            request.bearer_auth(token)
        } else {
            request.basic_auth(&self.profile.email, Some(token))
        };
        if let Some(body) = body {
            request = request.json(&body)
        }
        // Never retry a write, redirect an authenticated request, or log headers/body.
        let mut response = request.send().await.map_err(|_| {
            anyhow::anyhow!("Board API request failed or timed out; reload before retrying a move")
        })?;
        let status = response.status();
        if !status.is_success() {
            if status.as_u16() == 401 || status.as_u16() == 403 {
                bail!(
                    "Board API authorization failed (HTTP {}). Existing OAuth may lack Jira Software scopes; use acli authentication with an API token without scopes",
                    status.as_u16()
                )
            }
            bail!(
                "Board API HTTP {}; reload before retrying a move",
                status.as_u16()
            )
        }
        if method == Method::POST {
            ensure!(
                status.as_u16() == 204,
                "Unexpected move response; outcome unknown. Reload before retrying."
            );
            return Ok(Value::Null);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| anyhow::anyhow!("Board API response interrupted"))?
        {
            ensure!(
                bytes.len() + chunk.len() <= 32 * 1024 * 1024,
                "Board API response exceeds limit"
            );
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| anyhow::anyhow!("Invalid Board API JSON"))
    }
    async fn issues(&self, endpoint: &str, jql: &str) -> Result<Vec<Ticket>> {
        let mut all = Vec::new();
        let mut start = 0usize;
        loop {
            let v=self.request(Method::GET,endpoint,&[("startAt",start.to_string()),("maxResults","100".into()),("jql",jql.into()),("fields","summary,status,assignee,reporter,priority,issuetype,labels,description".into())],None).await?;
            let page: Vec<Ticket> = serde_json::from_value(v["issues"].clone())
                .context("Missing Board API issue page")?;
            ensure!(
                crate::workspace::valid_items(&page),
                "Board API returned invalid or duplicate keys"
            );
            let count = page.len();
            all.extend(page);
            start += count;
            if v["isLast"].as_bool() == Some(true)
                || v["total"]
                    .as_u64()
                    .is_some_and(|total| start as u64 >= total)
            {
                break;
            }
            ensure!(
                count > 0 && start < 100000,
                "Board API pagination did not finish"
            );
        }
        ensure!(
            crate::workspace::valid_items(&all)
                && all
                    .iter()
                    .map(|t| &t.key)
                    .collect::<std::collections::HashSet<_>>()
                    .len()
                    == all.len(),
            "Board changed during pagination; refresh again"
        );
        Ok(all)
    }
    pub async fn workspace(&self, scope: &str) -> Result<Workspace> {
        let (base, _) = crate::config::split_order(scope);
        let jql = format!("({base}) ORDER BY Rank ASC");
        // The public /board/issue resource is the whole saved-filter scope,
        // NOT membership in a team-managed page's Board section. Read the
        // actual manual issue-list and configured completed-item cutoff from
        // Jira's read-only web-board metadata, then hydrate/order with Rank.
        let query = [("rapidViewId", self.id.to_string())];
        let (model, plan) = tokio::try_join!(
            self.request(
                Method::GET,
                "greenhopper/1.0/rapidviewconfig/editmodel.json",
                &query,
                None
            ),
            self.request(
                Method::GET,
                "greenhopper/1.0/xboard/plan/backlog/data",
                &query,
                None
            )
        )?;
        ensure!(
            model["id"].as_u64() == Some(self.id),
            "Board metadata identity mismatch"
        );
        let board_query = membership_query(&model, &plan, base)?;
        let board_endpoint = format!("board/{}/issue", self.id);
        let backlog_endpoint = format!("board/{}/backlog", self.id);
        let (board, backlog) = tokio::try_join!(
            async {
                match board_query {
                    Some(q) => self.issues(&board_endpoint, &q).await,
                    None => Ok(Vec::new()),
                }
            },
            self.issues(&backlog_endpoint, &jql)
        )?;
        merge_membership(board, backlog)
    }
    pub async fn move_ticket(&self, key: &str, to_board: bool) -> Result<()> {
        let (endpoint, payload) = move_request(self.id, key, to_board)?;
        self.request(Method::POST, &endpoint, &[], Some(payload))
            .await?;
        Ok(())
    }
}
/// Derive visibility from actual issue-list membership and Jira's configured
/// cutoff. Evaluate `updated` in JQL on the SERVER, not against a local clock.
/// This read-only web metadata is version-sensitive: fail closed on unknown
/// shapes/strategies rather than substitute guessed statuses or all project work.
fn membership_query(model: &Value, plan: &Value, base: &str) -> Result<Option<String>> {
    ensure!(
        model["isSimpleBoard"].as_bool() == Some(true)
            && model["backlogStrategy"].as_str() == Some("ISSUE_LIST"),
        "Unsupported Board strategy; exact issue-list membership is required"
    );
    ensure!(
        plan["isTruncated"].as_bool() == Some(false)
            && plan["isIssueLimitExceeded"].as_bool() == Some(false),
        "Board membership response is truncated"
    );
    ensure!(
        model["boardIssueList"]["key"].as_str().is_some()
            && model["boardIssueList"]["key"] == plan["workViewIssueListKey"],
        "Board issue-list identity mismatch"
    );
    fn id(v: &Value) -> Result<u64> {
        let n = v
            .as_u64()
            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
            .context("Invalid Board membership ID")?;
        ensure!(n > 0, "Invalid Board membership ID");
        Ok(n)
    }
    let ids: std::collections::HashSet<u64> = plan["boardIssueIds"]
        .as_array()
        .context("Missing Board membership IDs")?
        .iter()
        .map(id)
        .collect::<Result<_>>()?;
    let issues = plan["issues"]
        .as_array()
        .context("Missing Board issue-list records")?;
    let mut active = Vec::new();
    let mut done = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for issue in issues {
        let n = id(&issue["id"])?;
        if !ids.contains(&n) {
            continue;
        }
        ensure!(seen.insert(n), "Duplicate Board membership record");
        if issue["done"]
            .as_bool()
            .context("Missing Board completion marker")?
        {
            done.push(n.to_string());
        } else {
            active.push(n.to_string());
        }
    }
    ensure!(
        seen.len() == ids.len(),
        "Incomplete Board membership records"
    );
    let mut clauses = Vec::new();
    if !active.is_empty() {
        clauses.push(format!("issue IN ({})", active.join(",")));
    }
    if !done.is_empty() {
        let cutoff = model["oldDoneIssuesCutoff"]
            .as_str()
            .context("Missing completed-issue visibility policy")?;
        ensure!(
            cutoff
                .strip_prefix('-')
                .and_then(|s| s.strip_suffix('d'))
                .and_then(|s| s.parse::<u32>().ok())
                .is_some_and(|days| days > 0 && days <= 36500),
            "Unsupported completed-issue visibility policy"
        );
        clauses.push(format!(
            "(issue IN ({}) AND updated >= {cutoff})",
            done.join(",")
        ));
    }
    if clauses.is_empty() {
        return Ok(None);
    }
    Ok(Some(format!(
        "({base}) AND ({}) ORDER BY Rank ASC",
        clauses.join(" OR ")
    )))
}
fn decode_credential(bytes: &[u8], oauth: bool) -> Result<String> {
    ensure!(bytes.len() < 1024 * 1024, "Credential envelope too large");
    let mut decoded = Vec::new();
    if bytes.starts_with(&[0x1f, 0x8b]) {
        GzDecoder::new(bytes)
            .take(1024 * 1024)
            .read_to_end(&mut decoded)
            .map_err(|_| anyhow::anyhow!("Invalid compressed acli credential"))?;
    } else {
        decoded.extend_from_slice(bytes)
    }
    let text = std::str::from_utf8(&decoded)
        .map_err(|_| anyhow::anyhow!("Unsupported acli credential encoding"))?
        .trim();
    let token = if let Ok(v) = serde_json::from_str::<Value>(text) {
        let keys: &[&str] = if oauth {
            &["access_token"]
        } else {
            &["api_token", "token", "access_token"]
        };
        keys.iter()
            .find_map(|key| v[*key].as_str())
            .context("Unsupported acli credential envelope")?
            .to_owned()
    } else {
        ensure!(!oauth, "Unsupported OAuth credential envelope");
        text.to_owned()
    };
    ensure!(
        !token.is_empty() && token.len() <= 16384 && token.bytes().all(|b| b.is_ascii_graphic()),
        "Invalid credential value"
    );
    Ok(token)
}
fn move_request(id: u64, key: &str, to_board: bool) -> Result<(String, Value)> {
    ensure!(id > 0, "Invalid board ID");
    data::require_key(key)?;
    let endpoint = if to_board {
        format!("board/{id}/issue")
    } else {
        format!("backlog/{id}/issue")
    };
    Ok((endpoint, json!({"issues":[key]})))
}
fn merge_membership(board: Vec<Ticket>, backlog: Vec<Ticket>) -> Result<Workspace> {
    let backlog_keys: std::collections::HashSet<_> =
        backlog.iter().map(|t| t.key.clone()).collect();
    // With explicit manual membership, overlap means the resources changed
    // during this read. Keep the previous snapshot, never guess a section.
    ensure!(
        !board.iter().any(|t| backlog_keys.contains(&t.key)),
        "Board/Backlog membership changed during fetch; refresh again"
    );
    let board: Vec<_> = board.into_iter().filter(Ticket::backlog_ticket).collect();
    let membership = board.iter().map(|t| t.key.clone()).collect();
    let mut items = board;
    items.extend(backlog.into_iter().filter(Ticket::backlog_ticket));
    let mut w = Workspace {
        items,
        board: Some(membership),
        complete: true,
        synced_at: crate::config::now(),
        ..Default::default()
    };
    w.reindex();
    Ok(w)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn board_membership_uses_actual_issue_list_and_configured_cutoff_not_status_names() {
        let model = json!({"isSimpleBoard":true,"backlogStrategy":"ISSUE_LIST","boardIssueList":{"key":"synthetic-list"},"oldDoneIssuesCutoff":"-7d"});
        let plan = json!({"isTruncated":false,"isIssueLimitExceeded":false,"workViewIssueListKey":"synthetic-list","boardIssueIds":["84",9],"issues":[{"id":"61","done":false},{"id":"84","done":false},{"id":"9","done":true}]});
        let q = membership_query(&model, &plan, "project = DEV")
            .unwrap()
            .unwrap();
        assert_eq!(
            q,
            "(project = DEV) AND (issue IN (84) OR (issue IN (9) AND updated >= -7d)) ORDER BY Rank ASC"
        );
        assert!(!q.contains("61"));
        assert!(!q.contains("status"));
        let mut bad = plan.clone();
        bad["isTruncated"] = json!(true);
        assert!(membership_query(&model, &bad, "project = DEV").is_err());
        let mut bad = model.clone();
        bad["oldDoneIssuesCutoff"] = json!("-7d OR assignee IS EMPTY");
        assert!(membership_query(&bad, &plan, "project = DEV").is_err());
        let mut bad = plan.clone();
        bad["workViewIssueListKey"] = json!("another-board");
        assert!(membership_query(&model, &bad, "project = DEV").is_err());
        let mut bad = plan.clone();
        bad["boardIssueIds"] = json!([84, 9, 12]);
        assert!(membership_query(&model, &bad, "project = DEV").is_err());
    }
    #[test]
    fn empty_board_is_valid_and_does_not_query_all_project_issues() {
        let model = json!({"isSimpleBoard":true,"backlogStrategy":"ISSUE_LIST","boardIssueList":{"key":"synthetic-list"}});
        let plan = json!({"isTruncated":false,"isIssueLimitExceeded":false,"workViewIssueListKey":"synthetic-list","boardIssueIds":[],"issues":[]});
        assert!(
            membership_query(&model, &plan, "project = DEV")
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn membership_is_stable_and_keeps_completed_board_rows() {
        let mut t = data::demo_tickets();
        t[0].key = "DEV-84".into();
        t[1].key = "DEV-9".into();
        t[1].fields["status"] = json!({"name":"Done","statusCategory":{"key":"done"}});
        t[2].key = "DEV-61".into();
        assert!(merge_membership(vec![t[0].clone(), t[2].clone()], vec![t[2].clone()]).is_err());
        let w = merge_membership(vec![t[0].clone(), t[1].clone()], vec![t[2].clone()]).unwrap();
        assert_eq!(
            w.items.iter().map(|t| t.key.as_str()).collect::<Vec<_>>(),
            ["DEV-84", "DEV-9", "DEV-61"]
        );
        assert!(w.board.as_ref().unwrap().contains("DEV-9"));
        assert!(!w.board.as_ref().unwrap().contains("DEV-61"));
    }
    #[test]
    fn moves_have_one_valid_key_and_correct_direction() {
        assert_eq!(
            move_request(17, "DEV-9", true).unwrap(),
            ("board/17/issue".into(), json!({"issues":["DEV-9"]}))
        );
        assert_eq!(
            move_request(17, "DEV-9", false).unwrap().0,
            "backlog/17/issue"
        );
        assert!(move_request(17, "DEV-1,DEV-2", true).is_err());
        assert!(move_request(0, "DEV-1", false).is_err());
    }
    #[test]
    fn global_oauth_uses_its_own_keyring_entry_and_bearer_transport() {
        let mut p = Profile {
            site: "synthetic.atlassian.net".into(),
            cloud_id: "synthetic-cloud".into(),
            account_id: "synthetic-account".into(),
            email: "alex@example.com".into(),
            auth_type: "oauth_global".into(),
        };
        assert!(p.is_oauth());
        assert_eq!(
            p.keyring_username(),
            "oauth:synthetic-cloud:synthetic-account"
        );
        p.auth_type = "oauth".into();
        assert!(p.is_oauth());
        assert_eq!(
            p.keyring_username(),
            "jira:synthetic-cloud:synthetic-account"
        );
        p.auth_type = "api_token".into();
        assert!(!p.is_oauth());
        assert_eq!(
            p.keyring_username(),
            "jira:synthetic-cloud:synthetic-account"
        );
    }
    #[test]
    fn auth_envelopes_never_need_oauth_refresh_implementation() {
        assert_eq!(
            decode_credential(
                br#"{"access_token":"synthetic-only","refresh_token":"unused"}"#,
                true
            )
            .unwrap(),
            "synthetic-only"
        );
        assert!(decode_credential(b"not-json", true).is_err());
        assert_eq!(
            decode_credential(b"synthetic-token\n", false).unwrap(),
            "synthetic-token"
        );
    }
}
