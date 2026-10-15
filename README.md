# Jirust

A fast Jira terminal client with **Mine, Available, Board and Backlog** views, instant cached search, inline images and confirmed ticket updates.

**Experimental.** Built for Jira Cloud on Linux/macOS. Exact Board/Backlog membership supports specific board types—not every Jira setup. See [support limits](docs/SUPPORT.md).

## 1. Install

You need [Rust **1.90+**](https://rustup.rs), Git, make, a C compiler and [Atlassian CLI (acli)](https://developer.atlassian.com/cloud/acli/guides/install-acli/). [Platform setup](docs/CONFIGURATION.md#platform-setup) covers more detail.

```sh
git clone https://github.com/ivanarena/jirust.git
cd jirust
make install
export PATH="$HOME/.local/bin:$PATH"
jirust --demo
```

The demo needs no Jira account. Add the PATH line to your shell configuration to keep it after restarting your terminal.

## 2. Connect Jira

```sh
cp .env.example .env
chmod 600 .env
${EDITOR:-vi} .env
```

For the recommended token setup, fill these entries in the private file:

| Setting | Your value |
|---|---|
| `JIRA_URL` | Your site, e.g. `https://example.atlassian.net` |
| `JIRA_EMAIL` | Your Jira account email |
| `JIRA_API_TOKEN` | An [API token](https://id.atlassian.com/manage-profile/security/api-tokens) **without scopes** |
| `JIRA_PROJECT` | Your project key, e.g. `DEV` |
| `JIRA_BOARD_ID` | Optional Board ID: the number after `/boards/` or `rapidView=` in its URL |

**Never commit the real file or put tokens in commands.** The prefix follows your project automatically. Leave other settings at their defaults.

```sh
jirust auth
jirust --read-only --no-cache --check
jirust --read-only
```

`auth` explicitly configures acli from your private file. Start read-only; omit `--read-only` when you want confirmed updates. Already authenticated? See [reusing a CLI login](docs/CONFIGURATION.md#authentication).

Run from the checkout, or pass `--env-file /absolute/path/to/.env` from elsewhere. [All settings and profile locations →](docs/CONFIGURATION.md)

## 3. Use it

| Key / gesture | Action |
|---|---|
| `Ctrl+1…4` / `Alt+1…4` / `F1…F4` | Switch views |
| `/` / type a number | Search / open a ticket |
| `↑↓`, `Tab`, mouse | Navigate |
| Click a row | Select and copy its Jira link |
| Double-click status, assignee or epic | Filter that field in the current view |
| `f` / `F6` | Filters / clear current filters |
| `E` | Toggle Rank/Epic sorting |
| `F5` | Refresh all views |
| `a`, `s`, `e`, `c`, `L`, `m` | Assign, status, edit, comment, link, move |
| `?` / `q` | Help / quit |

Updates require `y` confirmation and are never automatically retried. With a supported Board configured, membership and default ordering come from Jira—not guessed statuses. Without one, exact Board membership/moves are unavailable; unfinished tickets remain browsable.

Use `--no-copy-on-click` to disable clipboard writes, or `--no-images` to disable automatic images. Herdr is optional; Jirust works in an ordinary terminal of at least **60×16** cells.

## More

- [Configuration, authentication and troubleshooting](docs/CONFIGURATION.md)
- [Detailed controls, images and behavior](docs/USAGE.md)
- [Supported platforms and Jira boards](docs/SUPPORT.md) · [Security](SECURITY.md)
- Development: `make check && make build`. [Release checklist](docs/RELEASING.md)

[MIT license](LICENSE) · [Third-party notices](THIRD_PARTY_NOTICES.md)
