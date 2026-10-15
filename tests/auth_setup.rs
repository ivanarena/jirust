#![cfg(unix)]
use std::{fs, os::unix::fs::PermissionsExt, process::Command};

fn command(directory: &std::path::Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_jirust"));
    command
        .env_clear()
        .current_dir(directory)
        .env("HOME", directory);
    command
}

#[test]
fn auth_uses_environment_precedence_and_passes_token_only_on_stdin() {
    let directory = tempfile::tempdir().unwrap();
    let profile = directory.path().join(".env");
    fs::write(&profile, "JIRA_URL=https://example.atlassian.net\nJIRA_EMAIL=alex@example.com\nJIRA_API_TOKEN=file-token\n").unwrap();
    fs::set_permissions(&profile, fs::Permissions::from_mode(0o600)).unwrap();
    let mock = directory.path().join("acli");
    fs::write(&mock, "#!/bin/sh\n[ \"$*\" = 'jira auth login --site example.atlassian.net --email alex@example.com --token' ] || exit 1\n[ -z \"${JIRA_API_TOKEN+x}\" ] || exit 2\nvalue=$(/bin/cat)\n[ \"$value\" = 'process-token' ] || exit 3\n").unwrap();
    fs::set_permissions(&mock, fs::Permissions::from_mode(0o700)).unwrap();
    let before = fs::read(&profile).unwrap();
    let output = command(directory.path())
        .env("JIRA_API_TOKEN", "process-token")
        .args([
            "--backend",
            "acli",
            "--cli-bin",
            mock.to_str().unwrap(),
            "auth",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("process-token"));
    assert_eq!(fs::read(&profile).unwrap(), before);
}

#[test]
fn missing_credentials_without_terminal_fail_without_file_or_cli_changes() {
    let directory = tempfile::tempdir().unwrap();
    let profile = directory.path().join("new.env");
    for partial in [false, true] {
        let mut command = command(directory.path());
        if partial {
            command.env("JIRA_URL", "https://example.atlassian.net");
        }
        let output = command
            .args([
                "--env-file",
                profile.to_str().unwrap(),
                "--cli-bin",
                "/nonexistent/acli",
                "auth",
            ])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("interactive setup requires a terminal")
        );
        assert!(!profile.exists());
    }
}
