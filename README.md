# jiraui

A lightweight Jira terminal workspace: saved searches, filters, tickets, details, links and explicit update dialogs. Go + Bubble Tea; no Electron, browser engine, direct Jira HTTP client, or new authentication flow.

**Private personal project.** The application is standalone and can run in a Herdr pane or popup. Its backend delegates to `acli` or `jira-cli` using argument arrays, never a shell.

## Run

```sh
make install
jiraui                    # prefers an installed acli
jiraui --read-only        # safe evaluation against your real account
jiraui --check --limit 1  # read-only search/detail connectivity check, no UI
jiraui --demo             # synthetic tickets; no Jira requests or disk data cache
jiraui --backend jira     # use jira-cli's separately configured credentials
jiraui --project DEV
jiraui --jql 'project = DEV ORDER BY updated DESC'
```

Prerequisites: Go 1.24+ to build, a terminal of at least 60×18 cells, and either [Atlassian CLI](https://developer.atlassian.com/cloud/acli/guides/introduction/) or [jira-cli](https://github.com/ankitpokhrel/jira-cli). Ordinary external links open through `xdg-open` on Linux.

Authentication remains with the CLI. Restart jiraui after switching the CLI's active account/site. For acli, run `acli jira auth login` first. For jira-cli, run `jira init` and supply credentials in your shell/keyring as documented by that tool. jiraui never imports acli credentials into jira-cli, asks for a token, logs one, or saves one.

`--cli-bin /path/to/executable` overrides the selected binary. When selecting jira-cli, the adapter prefers `/usr/bin/jira` if present to avoid a local wrapper that silently routes commands to a different client.

## Workspace

- **My unfinished work**, all tickets, ranked backlog query, active sprint query, and status board.
- Search by text or issue key; filter loaded rows immediately, then press Enter for a server search.
- Combine project, assignee, status, reporter, type, priority, labels and sprint filters.
- `@me` and `unassigned` shortcuts; comma-separated field values; raw JQL for advanced queries.
- Save the current query as a named sidebar preset.
- Inspect descriptions (ADF rendered as text), comments, reporter, assignee, priority, labels, parent, subtasks, linked tickets, attachments and populated custom fields.
- Navigate ticket links internally with a Back stack; open external links/current ticket in the browser.
- Discover boards and select their actual active/future sprint ticket sets via acli.
- Assign, transition, edit summary/description/labels/type, comment, and link tickets. Every change is scoped to one explicit ticket and requires a separate `y` confirmation.
- Move tickets to a sprint through jira-cli. This action is unavailable with acli because that CLI has no corresponding command.

### Keyboard and mouse

| Key | Action |
| --- | --- |
| Tab / Shift+Tab | Focus sidebar, list, details |
| ↑↓ or j/k | Navigate list or scroll focused details |
| Enter | Focus ticket / activate choice |
| Esc / Backspace | Back from linked ticket / return to list |
| 1–5 | Built-in views |
| `/` | Local filter; Enter searches Jira |
| `f` | Structured filters |
| `:` | Raw JQL (overrides structured filters) |
| Ctrl+S | Save query; submit filter/editor dialog |
| `b` | Toggle status board for current query |
| `g` | Board → sprint picker (acli) |
| `r` | Refresh current query and ticket |
| `o` / `O` | Open ticket key internally / current ticket in browser |
| `l` | Ticket/external link picker (in details) |
| `C` | Load all comments (acli) |
| `a` / `t` / `e` / `c` | Assign / transition / edit / comment |
| `L` / `m` | Link tickets / move to sprint (jira-cli) |
| `!` / `?` / `q` | Full last CLI error / help / quit |

Mouse clicks select list rows, activate link lines and footer actions, and activate picker entries. Mouse wheel scrolls tickets or details. In board view, Left/Right (h/l) moves between status groups; press Enter for details before using `l` for links.

## Performance and local data

List/detail fetches run asynchronously. Selection is debounced so rapid navigation does not fetch every passing row. At most three CLI processes run concurrently; pending processes are cancelled when the app exits. Ticket details are reused in memory for two minutes. Query results appear from a local cache before being refreshed. There is **no background polling**: refresh explicitly with `r`.

Only requested data is fetched; queries are limited to 100 tickets by default (`--limit 1–100`). Reaching the limit is visibly reported; narrow the query rather than assuming the list is complete.

Caches live under `$XDG_CACHE_HOME/jiraui` (normally `~/.cache/jiraui`) in hashed CLI account/config namespaces. Files are mode 0600, directories 0700. Preferences and named queries live under `$XDG_CONFIG_HOME/jiraui/config.json`, also mode 0600. **Ticket content is still sensitive local data**; use `--no-cache` if your organization's policy prohibits persistence. No ticket fixtures from a real account are included in this repository.

## Honest v0.1 limitations

This is an initial implementation, not a complete Jira replacement:

- **Status board ≠ web board.** It groups loaded tickets by their actual status. Current CLIs do not expose web-board column mappings, swimlanes, or quick-filter configuration. No hardcoded Todo/Doing/Done mapping is substituted.
- **Backlog query ≠ exact web-board backlog.** It uses unfinished tickets with no sprint or a future sprint, ordered by Rank. Set a project or a saved/raw JQL matching your board's scope. Backlog re-ranking, drag-and-drop and exact board quick filters are not implemented because the current CLIs do not expose those operations.
- Sprint issue lists obtained through `g` use the CLI's actual board/sprint scope; only active/future sprints are discovered, up to 100 entries.
- Status suggestions come from returned transition metadata when available, otherwise loaded statuses. They are not asserted to be allowed transitions: Jira/CLI validates the requested transition. Required workflow fields or unsupported edits surface as errors, not silent success.
- Description editing currently replaces rich ADF formatting with plain text; the editor warns before confirmation. Rich descriptions are rendered as text, not a full WYSIWYG editor.
- Unsupported custom-field edits, priority edits on acli, project/type migration workflows, create-ticket forms, and board management are not implemented.
- Read-only real-account search, detail, board discovery and sprint discovery are smoke-tested. No active/future scrum sprint was available to live-test sprint ticket fetching; that adapter is tested with synthetic responses. **Real mutations are not performed during development**. Mutation argument construction and failure handling are tested with fake CLIs; validate permission-dependent workflows on a test ticket before relying on them.
- CLI subprocess/network latency still exists. No claims about a measured speedup over Jira web are made.

## Herdr

Run `jiraui` in a persistent pane/tab for normal work. Optional configuration snippet (choose a free shortcut):

```toml
[[keys.command]]
key = "prefix+alt+j"
type = "popup"
command = "jiraui"
width = "95%"
height = "95%"
```

This is a terminal app integration, not a fork of Herdr or its internal agent integrations.

## Development

```sh
make check        # vet and race-enabled tests, synthetic data only
make build
jiraui --demo
```

Backend adapters, model tests, query escaping, terminal sanitization, link navigation, confirmation/read-only gates, JSON response variants, account-scoped cache permissions, and fake mutation receipts are covered by tests. Actual Jira record changes must never be part of automated tests.
