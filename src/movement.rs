//! Confirmed, single-ticket CLI membership movement; no retries or workflow transitions.
use crate::{
    backend::{Action, Backend},
    config::{self, Settings},
    data,
    workspace::Workspace,
};
use anyhow::{Context, Result, ensure};
use clap::ValueEnum;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Destination {
    Board,
    Backlog,
}
impl Destination {
    pub fn name(self) -> &'static str {
        match self {
            Self::Board => "board",
            Self::Backlog => "backlog",
        }
    }
    pub fn source(self) -> &'static str {
        match self {
            Self::Board => "backlog",
            Self::Backlog => "board",
        }
    }
}
fn plan(
    workspace: &Workspace,
    id: u64,
    key: &str,
    to: Destination,
    read_only: bool,
) -> Result<Option<Action>> {
    ensure!(!read_only, "Read-only mode blocks ticket movement");
    data::require_key(key)?;
    ensure!(
        id > 0 && workspace.complete,
        "A complete, verified Board snapshot is required"
    );
    let membership = workspace
        .board
        .as_ref()
        .context("Board membership is not verified")?;
    ensure!(
        workspace.ticket(key).is_some(),
        "Ticket is not in the loaded Board/Backlog scope"
    );
    ensure!(
        !workspace
            .ticket(key)
            .is_some_and(crate::data::Ticket::is_epic),
        "Epic containers cannot be moved between Board and Backlog"
    );
    if membership.contains(key) == (to == Destination::Board) {
        return Ok(None);
    }
    Ok(Some(Action::new(to.name(), key, &id.to_string())))
}
pub async fn prepare(
    backend: &Backend,
    settings: &Settings,
    key: &str,
    to: Destination,
    read_only: bool,
) -> Result<Option<Action>> {
    ensure!(!read_only, "Read-only mode blocks ticket movement");
    data::require_key(key)?;
    let id = settings
        .board_id
        .context("Set JIRA_BOARD_ID or --board before moving tickets")?;
    let query = settings.query(config::BACKLOG, "", "", "")?;
    let mine = settings.query(config::MINE, "", "", "")?;
    let (workspace, _) = backend.workspace(&query, &mine, None).await?;
    plan(&workspace, id, key, to, read_only)
}
pub async fn execute(
    backend: &Backend,
    settings: &Settings,
    action: &Action,
    to: Destination,
    read_only: bool,
    confirmed: bool,
) -> Result<()> {
    ensure!(
        !read_only && confirmed,
        "Move requires confirmation and writable mode"
    );
    ensure!(
        action.kind == to.name()
            && settings
                .board_id
                .is_some_and(|id| action.value == id.to_string()),
        "Move destination changed since confirmation"
    );
    // The user may have spent time at the prompt. Revalidate membership and
    // scope before the one POST, and never repeat an uncertain POST.
    if let Some(current) = prepare(backend, settings, &action.key, to, read_only).await? {
        ensure!(
            current.key == action.key
                && current.kind == action.kind
                && current.value == action.value,
            "Move destination changed since confirmation"
        );
        backend.apply(&current).await?;
    }
    Ok(())
}
/// Shared confirmed TUI entry; no cached-state shortcut to the move POST.
pub async fn confirmed(
    backend: &Backend,
    settings: &Settings,
    action: &Action,
    read_only: bool,
) -> Result<()> {
    let to = match action.kind.as_str() {
        "board" => Destination::Board,
        "backlog" => Destination::Backlog,
        _ => anyhow::bail!("Unsupported membership destination"),
    };
    execute(backend, settings, action, to, read_only, true).await
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn tui_confirmed_moves_revalidate_scope_destination_and_already_completed_moves() {
        use std::sync::atomic::Ordering;
        let b = Backend::new("demo", None).unwrap();
        let s = Settings {
            project: "DEV".into(),
            board_id: Some(17),
            ..Default::default()
        };
        let a = prepare(&b, &s, "DEV-102", Destination::Board, false)
            .await
            .unwrap()
            .unwrap();
        let outside = Settings {
            project: "OTHER".into(),
            ..s.clone()
        };
        assert!(confirmed(&b, &outside, &a, false).await.is_err());
        let changed = Settings {
            board_id: Some(18),
            ..s.clone()
        };
        assert!(confirmed(&b, &changed, &a, false).await.is_err());
        assert!(confirmed(&b, &s, &a, true).await.is_err());
        assert_eq!(b.demo_moves.load(Ordering::SeqCst), 0);
        // Simulate a collaborator finishing the move while the dialog is open.
        b.apply(&a).await.unwrap();
        confirmed(&b, &s, &a, false).await.unwrap();
        assert_eq!(b.demo_moves.load(Ordering::SeqCst), 1);
        let before = b.get("DEV-102").await.unwrap().status();
        let a = prepare(&b, &s, "DEV-102", Destination::Backlog, false)
            .await
            .unwrap()
            .unwrap();
        confirmed(&b, &s, &a, false).await.unwrap();
        assert_eq!(b.demo_moves.load(Ordering::SeqCst), 2);
        assert_eq!(before, b.get("DEV-102").await.unwrap().status());
    }
    #[test]
    fn epic_containers_are_not_move_candidates() {
        let mut w = Workspace {
            items: data::demo_tickets(),
            complete: true,
            board: Some(Default::default()),
            ..Default::default()
        };
        w.items[0].fields["issuetype"] = serde_json::json!({"name":"Epic"});
        for to in [Destination::Board, Destination::Backlog] {
            assert!(
                plan(&w, 17, "DEV-101", to, false)
                    .unwrap_err()
                    .to_string()
                    .contains("Epic containers")
            );
        }
    }
    #[tokio::test]
    async fn confirmed_demo_move_is_one_ticket_and_does_not_change_status() {
        let backend = Backend::new("demo", None).unwrap();
        let settings = Settings {
            project: "DEV".into(),
            board_id: Some(1),
            ..Default::default()
        };
        let before = backend.get("DEV-102").await.unwrap().status();
        let a = prepare(&backend, &settings, "DEV-102", Destination::Board, false)
            .await
            .unwrap()
            .unwrap();
        assert!(
            execute(&backend, &settings, &a, Destination::Board, false, false)
                .await
                .is_err()
        );
        assert!(
            execute(&backend, &settings, &a, Destination::Board, true, true)
                .await
                .is_err()
        );
        execute(&backend, &settings, &a, Destination::Board, false, true)
            .await
            .unwrap();
        assert_eq!(before, backend.get("DEV-102").await.unwrap().status());
        assert!(
            prepare(&backend, &settings, "DEV-102", Destination::Board, false)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            prepare(&backend, &settings, "DEV-101", Destination::Backlog, false)
                .await
                .unwrap()
                .is_some()
        );
    }
    #[tokio::test]
    async fn preparation_blocks_read_only_invalid_keys_and_out_of_scope_tickets() {
        let backend = Backend::new("demo", None).unwrap();
        let settings = Settings {
            project: "DEV".into(),
            board_id: Some(1),
            ..Default::default()
        };
        for (key, read_only) in [
            ("DEV-102", true),
            ("DEV-102,DEV-103", false),
            ("DEV-999", false),
        ] {
            assert!(
                prepare(&backend, &settings, key, Destination::Board, read_only)
                    .await
                    .is_err()
            );
        }
        let workspace = Workspace {
            items: data::demo_tickets(),
            complete: true,
            ..Default::default()
        };
        assert!(plan(&workspace, 1, "DEV-102", Destination::Board, false).is_err());
    }
}
