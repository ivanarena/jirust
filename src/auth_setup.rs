use crate::environment::{Credentials, Environment};
use anyhow::{Context, Result, bail, ensure};
use std::{
    io::{self, IsTerminal, Write},
    path::Path,
    sync::Arc,
};

const KEYS: [&str; 3] = ["JIRA_URL", "JIRA_EMAIL", "JIRA_API_TOKEN"];

fn prompt(message: &str) -> Result<String> {
    print!("{message}");
    io::stdout().flush()?;
    let mut value = String::new();
    ensure!(io::stdin().read_line(&mut value)? != 0, "Setup cancelled");
    Ok(value.trim().to_owned())
}

fn confirmed(answer: &str) -> bool {
    answer.eq_ignore_ascii_case("y") || answer.eq_ignore_ascii_case("yes")
}

pub fn prepare(env: Environment, explicit: Option<&Path>) -> Result<Arc<Credentials>> {
    prepare_with(
        KEYS.map(|key| env.value(key)),
        KEYS.map(|key| std::env::var_os(key).is_some_and(|value| !value.is_empty())),
        io::stdin().is_terminal() && io::stdout().is_terminal(),
        |message, secret| {
            if secret {
                Ok(rpassword::prompt_password(message)?)
            } else {
                prompt(message)
            }
        },
        || Environment::auth_file(explicit),
    )
}

/// Saves prompted values and values from the file; exported variables stay out of the file.
fn prepare_with(
    mut values: [Option<String>; 3],
    exported: [bool; 3],
    interactive: bool,
    mut ask: impl FnMut(&str, bool) -> Result<String>,
    destination: impl FnOnce() -> Result<std::path::PathBuf>,
) -> Result<Arc<Credentials>> {
    let missing: Vec<_> = (0..KEYS.len()).filter(|&i| values[i].is_none()).collect();
    if missing.is_empty() {
        return Environment::credentials_from_values(values)?.context("Missing credentials");
    }
    ensure!(
        interactive,
        "Set JIRA_URL, JIRA_EMAIL and JIRA_API_TOKEN in the environment or a private env file; interactive setup requires a terminal"
    );
    if !confirmed(&ask(
        "Credentials are incomplete. Set missing values? [y/N] ",
        false,
    )?) {
        bail!("Setup cancelled");
    }
    for index in missing {
        let key = KEYS[index];
        let secret = key == "JIRA_API_TOKEN";
        let label = match key {
            "JIRA_URL" => {
                "Use the URL of your Jira Cloud site, for example https://your-site.atlassian.net.\nJIRA_URL: "
            }
            "JIRA_EMAIL" => "Use the email address of your Atlassian account.\nJIRA_EMAIL: ",
            "JIRA_API_TOKEN" => {
                "Create an API token without scopes at https://id.atlassian.com/manage-profile/security/api-tokens\nJIRA_API_TOKEN (hidden): "
            }
            _ => unreachable!(),
        };
        let value = ask(label, secret)?;
        ensure!(!value.is_empty(), "Setup cancelled: empty value");
        values[index] = Some(value);
    }
    let credentials =
        Environment::credentials_from_values(values.clone())?.context("Missing credentials")?;
    let values: Vec<_> = KEYS
        .into_iter()
        .zip(values.map(|value| value.expect("validated credentials")))
        .zip(exported)
        .filter_map(|(entry, exported)| (!exported).then_some(entry))
        .collect();
    let names: Vec<_> = values.iter().map(|(key, _)| *key).collect();
    let path = destination()?;
    if !confirmed(&ask(
        &format!("Save {} to {}? [y/N] ", names.join(", "), path.display()),
        false,
    )?) {
        bail!("Setup cancelled; no credentials saved");
    }
    let original = match std::fs::symlink_metadata(&path) {
        Ok(_) => Some(crate::private_fs::read_environment(&path, 65536)?),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => bail!("Cannot inspect environment file ({})", e.kind()),
    };
    let updated = update_dotenv(original.as_deref().unwrap_or_default(), &values)?;
    crate::private_fs::write_environment(&path, updated.as_bytes(), original.as_deref())
        .context("Cannot save private environment file")?;
    Ok(credentials)
}

fn update_dotenv(original: &[u8], values: &[(&str, String)]) -> Result<String> {
    let text = std::str::from_utf8(original).context("Invalid environment file encoding")?;
    let mut output = String::new();
    let mut record = String::new();
    for line in text.split_inclusive('\n') {
        record.push_str(line);
        let parsed =
            dotenvy::from_read_iter(record.as_bytes()).collect::<std::result::Result<Vec<_>, _>>();
        if let Ok(entries) = parsed {
            if !entries
                .iter()
                .any(|(key, _)| values.iter().any(|(replaced, _)| replaced == key))
            {
                output.push_str(&record);
            }
            record.clear();
        }
    }
    ensure!(
        record.is_empty(),
        "Invalid environment file syntax (values redacted)"
    );
    if !output.is_empty() && !output.ends_with('\n') {
        output.push('\n');
    }
    for (key, value) in values {
        let escaped = value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('$', "\\$");
        output.push_str(&format!("{key}=\"{escaped}\"\n"));
    }
    ensure!(output.len() <= 65536, "Environment file exceeds 64 KiB");
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dotenv_update_preserves_settings_and_comments_and_removes_duplicates() {
        let source = b"# profile\nJIRA_PROJECT=DEV\nJIRA_API_TOKEN=old\nexport JIRA_API_TOKEN='duplicate'\nOTHER=\"first\nsecond\"\n";
        let values = [
            ("JIRA_URL", "https://example.atlassian.net".into()),
            ("JIRA_EMAIL", "alex@example.com".into()),
            ("JIRA_API_TOKEN", "a$B\\c\"d'#".into()),
        ];
        let updated = update_dotenv(source, &values).unwrap();
        assert!(updated.starts_with("# profile\nJIRA_PROJECT=DEV\n"));
        assert!(updated.contains("OTHER=\"first\nsecond\"\n"));
        assert_eq!(updated.matches("JIRA_API_TOKEN=").count(), 1);
        let parsed: std::collections::BTreeMap<_, _> = dotenvy::from_read_iter(updated.as_bytes())
            .collect::<std::result::Result<_, _>>()
            .unwrap();
        for (key, value) in values {
            assert_eq!(parsed[key], value);
        }
        assert_eq!(parsed["OTHER"], "first\nsecond");
    }
    fn partial_environment() -> [Option<String>; 3] {
        [
            Some("https://example.atlassian.net".into()),
            Some("alex@example.com".into()),
            None,
        ]
    }
    #[cfg(unix)]
    #[test]
    fn missing_token_is_hidden_and_saved_then_loaded() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".env");
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut answers = ["y", "synthetic-token", "y"].into_iter();
        let mut hidden = 0;
        let credentials = prepare_with(
            partial_environment(),
            [false; 3],
            true,
            |_, secret| {
                hidden += usize::from(secret);
                Ok(answers.next().unwrap().into())
            },
            || Ok(path.clone()),
        )
        .unwrap();
        assert_eq!(hidden, 1);
        assert_eq!(credentials.token, "synthetic-token");
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(dir.path()).unwrap().permissions().mode() & 0o777,
            0o755
        );
        assert!(Environment::load(Some(&path)).is_ok());
        let bytes = std::fs::read(&path).unwrap();
        let parsed: std::collections::BTreeMap<_, _> = dotenvy::from_read_iter(bytes.as_slice())
            .collect::<std::result::Result<_, _>>()
            .unwrap();
        assert_eq!(parsed["JIRA_API_TOKEN"], "synthetic-token");
    }
    #[test]
    fn missing_credentials_show_help_urls_and_accept_custom_site() {
        let mut answers = [
            "y",
            "https://example.atlassian.net/",
            "alex@example.com",
            "synthetic-token",
            "n",
        ]
        .into_iter();
        let mut prompts = Vec::new();
        let result = prepare_with(
            [None, None, None],
            [false; 3],
            true,
            |label, secret| {
                prompts.push((label.to_owned(), secret));
                Ok(answers.next().unwrap().into())
            },
            || Ok("unused.env".into()),
        );
        assert!(result.is_err());
        assert!(prompts[1].0.contains("https://your-site.atlassian.net"));
        assert!(
            prompts[2]
                .0
                .contains("email address of your Atlassian account")
        );
        assert!(
            prompts[3]
                .0
                .contains("https://id.atlassian.com/manage-profile/security/api-tokens")
        );
        assert!(prompts[3].0.contains("without scopes"));
        assert!(prompts[3].1);
        assert!(!prompts[1].1);
        assert!(!prompts[2].1);
    }
    #[test]
    fn empty_jira_url_cancels_without_write() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".env");
        let mut answers = ["y", ""].into_iter();
        assert!(
            prepare_with(
                [
                    None,
                    Some("alex@example.com".into()),
                    Some("synthetic-token".into()),
                ],
                [false; 3],
                true,
                |_, _| Ok(answers.next().unwrap().into()),
                || Ok(path.clone()),
            )
            .is_err()
        );
        assert!(!path.exists());
    }
    #[cfg(unix)]
    #[test]
    fn exported_values_are_not_saved_and_file_values_are_kept() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".env");
        std::fs::write(&path, "JIRA_URL=https://example.atlassian.net\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let mut answers = ["yes", "alex@example.com", "YES"].into_iter();
        let mut prompts = Vec::new();
        prepare_with(
            [
                Some("https://example.atlassian.net".into()),
                None,
                Some("exported-token".into()),
            ],
            [false, false, true],
            true,
            |label, _| {
                prompts.push(label.to_owned());
                Ok(answers.next().unwrap().into())
            },
            || Ok(path.clone()),
        )
        .unwrap();
        assert!(prompts[2].starts_with("Save JIRA_URL, JIRA_EMAIL to "));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("exported-token"));
        assert!(!text.contains("JIRA_API_TOKEN"));
        assert!(text.contains("JIRA_EMAIL=\"alex@example.com\""));
    }
    #[test]
    fn complete_credentials_skip_prompts_and_writes() {
        let mut env = partial_environment();
        env[2] = Some("synthetic-token".into());
        assert!(
            prepare_with(
                env,
                [false; 3],
                false,
                |_, _| panic!("no prompt"),
                || panic!("no destination")
            )
            .is_ok()
        );
    }
    #[test]
    fn noninteractive_missing_credentials_fail_without_prompt() {
        assert!(
            prepare_with(
                partial_environment(),
                [false; 3],
                false,
                |_, _| panic!("no prompt"),
                || panic!("no destination")
            )
            .is_err()
        );
    }
    #[test]
    fn cancellation_and_invalid_values_do_not_write() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".env");
        for answers in [
            vec!["n"],
            vec!["y", "synthetic-token", "n"],
            vec!["y", ""],
            vec!["y", "invalid token"],
        ] {
            let mut answers = answers.into_iter();
            assert!(
                prepare_with(
                    partial_environment(),
                    [false; 3],
                    true,
                    |_, _| Ok(answers.next().unwrap().into()),
                    || Ok(path.clone())
                )
                .is_err()
            );
            assert!(!path.exists());
        }
    }
    #[test]
    fn missing_explicit_file_is_allowed_only_for_auth() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.env");
        assert!(Environment::load(Some(&path)).is_err());
        assert!(Environment::load_for_auth(Some(&path)).is_ok());
        assert_eq!(Environment::auth_file(Some(&path)).unwrap(), path);
    }
    #[cfg(unix)]
    #[test]
    fn environment_writer_rejects_unsafe_files() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        let link = dir.path().join(".env");
        let original = Some(b"original".as_slice());
        crate::private_fs::write_environment(&target, b"original", None).unwrap();
        assert!(crate::private_fs::write_environment(&target, b"new", None).is_err());
        assert!(crate::private_fs::write_environment(&target, b"new", Some(b"changed")).is_err());
        symlink(&target, &link).unwrap();
        assert!(crate::private_fs::write_environment(&link, b"new", original).is_err());
        std::fs::remove_file(&link).unwrap();
        std::fs::hard_link(&target, &link).unwrap();
        assert!(crate::private_fs::write_environment(&link, b"new", original).is_err());
        std::fs::remove_file(&link).unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(crate::private_fs::write_environment(&target, b"new", original).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"original");
    }
    #[test]
    fn invalid_or_oversized_dotenv_is_rejected() {
        assert!(update_dotenv(b"BAD=\"unterminated", &[]).is_err());
        assert!(update_dotenv(&vec![b'#'; 65537], &[]).is_err());
    }
}
