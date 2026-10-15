# Configuration

Jirust has no built-in company, account, project, ticket prefix, board ID or browser site. The tracked `.env.example` is synthetic; set your own scope. The four views and authoritative Jira membership/Rank rules remain the same for every account.

## Platform setup

For the short install/connect/run sequence, start with the [README](../README.md).

- **Linux:** install [Rust 1.90+](https://rustup.rs), Git, make, a C compiler and [acli](https://developer.atlassian.com/cloud/acli/guides/install-linux/). On Debian/Ubuntu, `sudo apt-get install build-essential git` provides the build tools.
- **macOS:** with Homebrew installed, run `brew tap atlassian/homebrew-acli` and `brew install acli rust`; install Xcode Command Line Tools if a compiler is missing. See the [official acli guide](https://developer.atlassian.com/cloud/acli/guides/install-macos/).

`make install` builds Jirust into `~/.local/bin`; add that directory to PATH. Leave `JIRA_CLI_BIN` empty for native executable discovery, especially when moving between Linux and macOS. Herdr is optional. The terminal must be at least 60×16 cells.

## Authentication

**Recommended on either OS:** fill `JIRA_URL`, `JIRA_EMAIL` and `JIRA_API_TOKEN` together in a private 0600 env file, using an API token **without scopes**, then run `jirust auth`. This explicitly configures acli; the token goes through stdin, never argv. Validate with `jirust --read-only --no-cache --check` before starting the TUI.

**Interactive setup:** Run `jirust auth` in a terminal. If credentials are incomplete, Jirust offers to set the missing values. At the `JIRA_URL` prompt, enter the URL of your Jira Cloud site, for example `https://your-site.atlassian.net`. There is no default site. The API token stays hidden. Jirust validates the completed credentials and asks for confirmation before it saves them. It then authenticates acli. If authentication fails, the saved credentials remain available for correction.

The destination follows the dotenv selection order below. If no file exists, Jirust creates the personal configuration `.env`. An explicitly selected absent file is permitted for `auth` only. Jirust preserves unrelated settings and comments. It saves the values that you enter and the values from the file in a private `0600` file. It does not save exported variables, which still take precedence. The confirmation prompt names the keys that Jirust will save. If the file changes before Jirust saves it, Jirust stops and saves nothing. Cancellation saves nothing. Without a terminal, incomplete credentials cause an error instead of a prompt.

**Reuse a CLI login:** leave all three authentication variables empty and skip `jirust auth`. Ordinary operations use that login. The optional Board/image/field HTTP bridges can reuse the active acli keyring entry on **Linux only**, with `secret-tool` installed. Some OAuth grants lack Jira Software scopes even if ordinary issue reads work; use the explicit token setup if needed. **macOS HTTP bridges require the private token setup**; Jirust does not read macOS Keychain.

Startup never silently switches accounts. CLI authentication must match the configured site/email; restart after changing authentication. Conflicting acli metadata in standard/native config locations fails closed instead of guessing. Direct HTTP remains restricted to the approved pinned Cloud endpoints; browser overrides do not change authentication.

## Initial project setup

If no project or JQL scope is configured, Jirust lists the active projects visible to your Jira account. It uses `acli jira project list --paginate --json`. Each entry shows the project key and name. Use the arrow keys and Enter, or click an entry. Jirust saves the selected project in `config.json` and loads its tickets. This also works in read-only mode; no Jira records change.

Press `r` to retry or refresh the list. Select **Enter a project or JQL manually** for manual setup. Empty results and CLI errors keep this option available. Jirust skips list entries that have no valid project key or name. Project discovery requires the acli backend. Other backends support manual setup. Existing project or JQL configuration keeps the advanced form under `g`.

## Precedence and profiles

Explicit CLI flags win over exported environment variables, then the selected dotenv file, then saved/default settings. An empty exported variable or an empty dotenv value supplies no override. Scope/project/prefix/JQL/board/name/view settings may be remembered for later TUI launches; credentials, browser overrides and refresh intervals are not written to settings. `--check`, subcommands and demo do not save those settings.

Select a dotenv file in this order:

1. `--env-file /private/path/work.env`
2. exported `JIRUST_ENV_FILE=/private/path/work.env`
3. `./.env`
4. `.env` in the [platform-native Jirust config directory](#storage-locations).

An explicitly selected missing/invalid file fails rather than falling back to another account. Default-file inspection errors also fail; only an absent default file permits trying the next location. `JIRUST_ENV_FILE` is a bootstrap selector from the process environment, not a variable inside another dotenv file. Keep files owned by you, regular, **0600**, no symlinks/hardlinks, and at most **64 KiB**. A normal owned 0755 project directory is supported; its permissions are never changed. The immediate parent must not be writable by other users. Dotenv values are not exported globally or to CLI children. Demo ignores personal environment settings; explicit demo flags still work.

```sh
cp .env.example /private/path/work.env
chmod 600 /private/path/work.env
# Edit with your own project, optional board and authentication settings.
jirust --env-file /private/path/work.env --read-only --no-cache --check
jirust --env-file /private/path/work.env
```

Profiles select settings, **not accounts**. Jirust never silently switches the CLI's active login. Use explicit `jirust --env-file /private/path/work.env auth` only when intentionally changing authentication, then restart. Account/site mismatch checks and account-private caches still apply.

## Storage locations

| Platform | Settings / private env directory | Account-scoped cache root |
|---|---|---|
| Linux | `${XDG_CONFIG_HOME:-~/.config}/jirust` | `${XDG_CACHE_HOME:-~/.cache}/jirust/rust/<account-hash>` |
| macOS | `~/Library/Application Support/jirust` | `~/Library/Caches/jirust/rust/<account-hash>` |

Settings are `config.json`; the optional private env is `.env`. Account/site determines cache identity; unknown identity disables persistence. Files are private and directories owned/private. `--no-cache` disables ticket/image persistence; demo does not save settings/cache or migrate personal configuration. When upgrading from the former app name, quit the old process first; safe migration never overwrites an existing new directory. See [storage/security boundaries](../SECURITY.md).

## Settings reference

| Environment variable | CLI flag | Meaning/default |
|---|---|---|
| `JIRA_URL` | — | Explicit token's HTTPS Atlassian Cloud origin; e.g. `https://example.atlassian.net`. |
| `JIRA_EMAIL` | — | Explicit token's account email. |
| `JIRA_API_TOKEN` | — | Private token; never supply it as an argument. Set all three authentication variables together, or leave all empty to reuse CLI authentication. |
| `JIRA_BROWSE_URL` | `--browse-url` | Browser/clipboard base only; default pinned CLI/Board site or ticket site/context. |
| `JIRA_PROJECT` | `--project` | Your project key; no built-in live project. |
| `JIRA_PREFIX` | `--prefix` | Numeric-search ticket prefix; empty follows project. Explicit prefixes are Jira key prefixes: letter first, then ASCII letters/digits/underscores, at most 64 bytes. |
| `JIRA_BOARD_ID` | `--board` | Positive actual Jira Board ID; opt-in exact-membership bridge. Empty leaves saved/default board settings. |
| `JIRA_JQL` | `--jql` | Optional custom scope JQL. Jira remains authoritative for membership. |
| `JIRA_SCOPE_NAME` | `--scope-name` | Optional scope display label, at most 128 bytes without controls; does not change JQL/membership. |
| `JIRA_VIEW` | `--view` | `mine`, `available`, `board`, `backlog`; otherwise restore saved view. |
| `JIRA_BACKEND` | `--backend` | `auto` (default), `acli`, `jira`, `demo`. |
| `JIRA_CLI_BIN` | `--cli-bin` | Optional executable path; otherwise native discovery/PATH. Do not copy Linux paths to macOS. |
| `JIRA_READ_ONLY` | `--read-only` | Block update entry and confirmation; default false. |
| `JIRA_NO_CACHE` | `--no-cache` | Disable ticket/image persistence; default false. |
| `JIRA_LIMIT` | `--limit` | Legacy cache limit, 1..2000/default 500; does not truncate complete acli pagination. Legacy jira-cli remains partial/capped at 100. |
| `JIRA_REFRESH_SECONDS` | `--refresh-seconds` | Periodic background sync, **15..86400 seconds**, default **60**. |
| `JIRA_REFRESH_RETRY_SECONDS` | `--refresh-retry-seconds` | Background sync retry after failure, same bounds, default **120**. Never a mutation retry. |
| `JIRA_ATTACHMENT_DIR` | `--attachment-dir` | Optional manually downloaded raster root; otherwise account-private default. |
| `JIRA_IMAGE_PROTOCOL` | `--image-protocol` | `auto`, `kitty`, `halfblocks`; default auto. |
| `JIRA_NO_IMAGES` | `--no-inline-images` / `--no-images` | Disable automatic inline images; default false. Explicit `--images` overrides environment disabling. |
| `JIRA_NO_COPY_ON_CLICK` | `--no-copy-on-click` | Disable row-click clipboard writes; default false. Explicit `--copy-on-click` overrides environment disabling. |

Boolean values: `true`/`false`, `1`/`0`. Invalid values fail without echoing dotenv contents. A new explicit project does not inherit another project's environment prefix/JQL/board/display label unless supplied explicitly. Choosing a project resets old custom prefix/JQL/label unless overridden; an actual project change clears the saved board. Changing JQL clears an old scope label unless a new label is supplied.

Refresh intervals only affect periodic reads. F5 and successful confirmed writes still fully refresh all four views immediately. Existing read-only, confirmation and no-automatic-write-retry behavior is unchanged.

## Browser links are not API configuration

Examples:

```dotenv
JIRA_BROWSE_URL=https://example.atlassian.net
# Or a browser-only alias/context (do not include /browse/KEY):
# JIRA_BROWSE_URL=https://jira.example.org/jira
```

The second example generates `https://jira.example.org/jira/browse/DEV-123` for a synthetic ticket. HTTP(S), optional port and plain context paths are supported; credentials, query strings, fragments and encoded context paths are rejected. Prefer HTTPS. The override applies consistently to generated open-ticket/attachment links and row-click copies, including cached tickets; it is session-only and not serialized in ticket caches. Original descriptions and unrelated external links are not rewritten.

No token, OAuth bearer or attachment request is ever sent to this browser base. Board/image/field HTTP remains pinned to the independently validated CLI/token site, with fixed approved endpoints, no redirects and existing bounds. Without an override, server `/jira/rest/...` metadata preserves `/jira` in browser links. OAuth gateway URLs are not guessed into browser links; use the pinned profile site or supply the browser base.

A custom browser host/context is **not** support for arbitrary authenticated API hosts, Jira Data Center membership, other board strategies or universal terminal/macOS parity. See the [support matrix](SUPPORT.md).

## Troubleshooting

| Problem | What to check |
|---|---|
| `jirust: command not found` | Add `$HOME/.local/bin` to PATH, or use `./bin/jirust` from a built checkout. |
| No project/scope or wrong env | Set `JIRA_PROJECT`; use `--env-file /absolute/path` when launching elsewhere. |
| Env file rejected | Owned regular 0600 file, no symlink/hardlink, parent not writable by others; only absent default files permit fallback. |
| Account mismatch / HTTP 401 | Check the three authentication settings and permissions. Run `jirust auth` only for the intended account, then restart. OAuth can lack Software scopes. |
| Unsupported Board metadata | Check the actual Board ID and support matrix; never substitute status guesses for membership. |
| Empty list after filtering | F6 clears the current view; shown/scope counts distinguish filtering from missing data. |
| Image/clipboard trouble | Try `--image-protocol halfblocks`, `--no-images` or `--no-copy-on-click`; see [usage](USAGE.md). |
