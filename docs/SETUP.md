# Setup details

[Quick start](../README.md) · [All settings](CONFIGURATION.md) · [Support limits](SUPPORT.md)


Building requires Rust **1.90+** and `make`. Live use requires `acli` (preferred) or `jira-cli`, authentication, and a UTF-8 terminal of at least **60×16** cells. Herdr is optional; any suitable terminal works.

```sh
make check
make install
jirust
jirust --demo
jirust --project DEV
jirust --project DEV --read-only --no-cache --check
```

Explicit scope flags are remembered for subsequent launches (except `--check`/demo). `--project` infers the matching prefix and replaces a previous scope unless `--prefix`/`--jql` are supplied. Automatic backend selection prefers the installed `acli`; override with `--backend acli|jira|demo` and optionally `--cli-bin /path/to/binary`. Authentication validity is still the CLI's responsibility. `/usr/bin/jira` is preferred over wrappers.

The acli workspace fetch uses **pagination**, not the old 500-row limit, so the complete scoped dataset is fetched within the documented resource budgets. Oversized scopes fail explicitly; they are never presented as complete truncated membership. `--limit` remains for legacy cache migration. The jira-cli adapter is still capped at 100 and explicitly marks a partial scope; prefer acli for complete datasets. Numeric lookup can open completed tickets; API Board snapshots retain completed tickets returned by Jira.

`--read-only` blocks both update entry and confirmation. Writes are never automatically retried after failure/timeout; the outcome may be unknown. Reload before retrying. Development mutation tests use only synthetic DEV fixtures.

Platform-native locations:

| Platform | Settings / private env directory | Ticket cache directory |
|---|---|---|
| Linux | `${XDG_CONFIG_HOME:-~/.config}/jirust` | `${XDG_CACHE_HOME:-~/.cache}/jirust/rust/<account-hash>` |
| macOS | `~/Library/Application Support/jirust` | `~/Library/Caches/jirust/rust/<account-hash>` |

Settings are `config.json`; the optional private env is `.env`. CLI-reported identity/site determines the cache namespace; unknown identity disables persistent ticket caching. Files are 0600, directories 0700. Cache/config opens validate Unix ownership, private permissions, regular files and no-follow leaf paths; symlinks/hardlinks are rejected rather than trusted. Atomic writes and nonblocking per-directory locks protect cache updates/quotas across client processes. `--no-cache` disables ticket persistence; demo saves neither settings nor caches. Restart after switching CLI account/site.

When upgrading from the old app name, **quit the old process first**, then start `jirust`. On first use, legacy config/cache directories are renamed to the new directories, preserving the private environment file, settings, account namespaces and attachments without rewriting their contents. An existing new directory is never overwritten. Demo does not migrate personal configuration.

## Local environment configuration

Copy the synthetic `.env.example` to **`.env` in the project directory**, or to **`.env` in the platform-specific config directory above**, and set permissions to **0600**. Set your own project/prefix, optional Board ID and settings there. Leave the three authentication variables empty to reuse your CLI login, or supply your own HTTPS Cloud URL, email and API token without scopes together. The real file is plaintext: protect it and never commit/share it. The tracked template contains no credentials. Alternatively use a private `./.env`, or select an explicit file with `--env-file /path/to/.env`.

Precedence: explicit CLI flags → exported environment variables → selected dotenv file → saved/default settings. File discovery: explicit `--env-file`, otherwise exported `JIRUST_ENV_FILE`, otherwise `./.env`, otherwise the user config dotenv file. Explicit missing/invalid profiles fail instead of falling back to another account. File values are not exported into the process or child environments. Demo ignores local/company configuration entirely. A new explicit project does not inherit another project's environment Board/prefix/JQL.

```sh
jirust auth                            # token piped privately to acli stdin
jirust --read-only --no-cache --check  # validate without changing tickets
jirust                                # start the TUI
```

`JIRA_URL`, `JIRA_EMAIL` and `JIRA_API_TOKEN` must be supplied together. Only HTTPS Atlassian Cloud origins are accepted; credential-bearing URLs, redirects and arbitrary hosts are rejected. The Board bridge uses the local token directly; ordinary operations still use acli authentication. `auth` explicitly updates acli's own keyring/profile from that token; startup never silently switches CLI accounts. A mismatched site/email fails closed and asks you to authenticate explicitly. Restart after editing authentication variables.

Configure scope with `JIRA_PROJECT`, `JIRA_PREFIX`, `JIRA_BOARD_ID`, `JIRA_JQL`, `JIRA_SCOPE_NAME` and `JIRA_VIEW`. Optional `JIRA_BROWSE_URL` controls generated browser/clipboard links, including context paths, **never authenticated API requests**. Periodic reads use `JIRA_REFRESH_SECONDS` (default 60) and `JIRA_REFRESH_RETRY_SECONDS` (default 120), each 15..86400 seconds; confirmed successful writes still trigger an immediate full refresh. `JIRA_NO_IMAGES` and `JIRA_NO_COPY_ON_CLICK` control side effects; explicit `--images` / `--copy-on-click` can re-enable them. Backend/path/cache/image settings remain configurable too. See the [complete variable/flag reference and profile examples](CONFIGURATION.md). Boolean values are `true` / `false` (also `1` / `0`). Tokens are never placed in command arguments, debug output, settings JSON or caches; dotenv/auth errors redact values. Do not combine `--check` with a subcommand.

## Linux prerequisites

Install [Rust 1.90+](https://rustup.rs) and follow the [official acli Linux instructions](https://developer.atlassian.com/cloud/acli/guides/install-linux/).

On Debian/Ubuntu, install the build tools first:

```sh
sudo apt-get update
sudo apt-get install build-essential git
```

On other distributions, install Git, make and a C compiler with your package manager. Then follow the README's clone/build/configure steps. The recommended private-token setup does not require a Linux keyring helper.

## macOS setup (Intel or Apple Silicon)

Install Homebrew first if needed, then install Rust and Atlassian CLI using [Atlassian's macOS installation guide](https://developer.atlassian.com/cloud/acli/guides/install-macos/):

```sh
brew tap atlassian/homebrew-acli
brew install acli rust
# In the cloned Jirust repository:
make check
make install
export PATH="$HOME/.local/bin:$PATH"
cp .env.example .env
chmod 600 .env
${EDITOR:-vi} .env
```

In the private file, fill the URL/email/token/project/Board entries with your own values. Use an API token **without scopes**. Leave `JIRA_CLI_BIN` empty to find Homebrew's `acli` on PATH; never copy a Linux-specific `/usr/bin/acli` setting to a Mac. Then:

```sh
jirust --env-file "$PWD/.env" auth
jirust --env-file "$PWD/.env" --read-only --no-cache --check
jirust --env-file "$PWD/.env"
# Later, one confirmed membership move:
jirust --env-file "$PWD/.env" move DEV-123 --to board
```

Replace `DEV-123` with the actual one-ticket key. The explicit file path works from any directory when changed to an absolute path. Alternatively keep the file at `~/Library/Application Support/jirust/.env` for automatic discovery. Add `~/.local/bin` to your shell's PATH permanently if desired. Browser links use macOS `open`; image previews have a Unicode fallback. No GNU `install -D` or Linux desktop integration is required.

**Mac Board/image/field HTTP access requires a local API token.** The keyring reuse fallback below is Linux-only; Jirust does not read macOS Keychain itself. `jirust auth` delegates secure CLI credential storage to acli. Metadata can be read from acli's standard `~/.config/acli` or native platform config directory; conflicting active profiles fail closed instead of guessing an account.

CI runs build, lint, unit tests and a synthetic read-only demo on Linux and macOS. These tests require no credentials and make no real Jira writes. A successful build/demo does **not** establish live Jira or terminal-graphics parity on every Mac; verify your own account with the read-only check above.

## Board API authentication without a local token

Linux `secret-tool` must be installed when using the keyring fallback. The bridge reads only the active acli Jira profile's keyring entry; credentials are kept in memory. Both site OAuth and global OAuth are supported; global OAuth uses its own keyring entry and requires matching active global/Jira identities. Some OAuth grants work for ordinary issues but lack Jira Software API scopes (HTTP 401 `scope does not match`). A successful browser/global OAuth login alone does not grant missing scopes. The app cannot grant these permissions. Use a supported acli API-token login **without scopes**, supplying the token privately through stdin: `acli jira auth login --site mysite.atlassian.net --email you@example.com --token`. Do not put tokens in command arguments, this repository, or chat. Restart afterward, then validate read-only with `jirust --board 17 --read-only --check`. Live membership/parity must be verified for each board after authorization succeeds.

CLI jobs use argument arrays, asynchronous execution, concurrency limits, and timeouts. Rendering is event-driven; network updates happen independently of local navigation. Untrusted terminal controls are stripped and browser links must be ordinary HTTP(S) URLs. Only the approved Board bridge, read-only verified raster attachment downloader, and story-point field metadata lookup use direct HTTP with a private environment token or current acli keyring credentials. It pins HTTPS origins, disables redirects, bounds concurrency/response sizes, and never logs tokens or writes them into app settings/caches. The only user-approved plaintext credential storage is the private local environment file. Account/site/auth changes require restart. OAuth login and refresh remain delegated to acli.

## Common setup problems

| Problem | What to do |
|---|---|
| `jirust: command not found` | Add `$HOME/.local/bin` to PATH, or run `./bin/jirust` from the checkout after building. |
| acli is missing | Install it using the official platform guide; leave `JIRA_CLI_BIN` empty unless you need an explicit executable. |
| No project/scope | Set `JIRA_PROJECT` in your private env file; use `--env-file /absolute/path` if launching elsewhere. |
| Env file rejected | Use an owned regular 0600 file, without symlinks/hardlinks, in an owned directory not writable by others. |
| Account/site mismatch or HTTP 401 | Check all three authentication values and CLI permissions. Use explicit `jirust auth` only for the intended account, then restart; OAuth may lack Software scopes. |
| Unsupported Board metadata | Check the Board ID and support matrix. Do not substitute guessed status filters for membership. |
| Empty list after filtering | F6 clears the current view's filters; check the shown/scope counts before assuming tickets are missing. |

## Implementation map

`src/environment.rs` loads private local configuration; `src/movement.rs` validates confirmed CLI moves; `src/backend.rs` delegates ordinary operations to CLIs; `src/board_api.rs` implements the membership/image HTTP bridges; `src/images.rs` provides bounded full-width sliced rasters and private quota-limited caching; `src/estimates.rs` identifies authoritative story-point fields; `src/workspace.rs` provides the shared cached dataset/local filtering; `src/config.rs` defines scope and the four views; `src/app.rs` handles state/actions; `src/render.rs` provides the two-pane layout; `src/markdown.rs`, `src/data.rs`, and `src/input.rs` handle rich text, sanitization, and editing. The previous Go prototype remains in Git history.
