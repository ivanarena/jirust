# jiraui

A lightweight **Rust + Ratatui** Jira terminal client. Two panes, no browser engine, and no new authentication setup: Jira operations run through your existing **acli** or **jira-cli**.

```text
jiraui · scope       ^1 Mine     ^2 Available     ^3 Backlog
/ Search Mine        Assignee: Me ▾              Status: Any ▾
╭ Tickets ──────────────────╮╭ Ticket ───────────────────────╮
│ Key · Status · Assignee  ││ Description, reporter, links  │
│ Summary                 ││ Attachments and comments      │
╰─────────────────────────╯╰───────────────────────────────╯
```

## Three predefined views

These are **assignment filters over the same combined backlog-page ticket scope**, not different board modes:

| Shortcut | View | Filter |
|---|---|---|
| Ctrl+1 | **Mine** | Assigned to the current CLI user |
| Ctrl+2 | **Available** | Unassigned |
| Ctrl+3 | **Backlog** | All assignees; includes both Board and Backlog work |

There is no separate Board tab, no backlog-only status list, no sprint workflow, and no required status-mapping setup. Both Mine and Available include tickets from either part of the backlog page. Tabs change assignment filtering; text/status filters are retained. Old tab indices/settings are migrated.

Click tabs if preferred. **Alt+1…3 / F1…F3** are fallbacks for terminals that cannot distinguish Ctrl+number from ordinary digits. Additional assignee choices select Mine, Available, or a filtered Backlog rather than silently contradicting the selected view. Custom saved top views are no longer exposed.

The existing personal project/JQL configuration is reused. All three views use that scope and exclude completed work. The installed CLIs do not expose a web board's saved filter/configuration automatically: without an explicit saved-filter scope, the fallback is unfinished project work, **not a guaranteed exact replica of an arbitrary web board**. Optional advanced project/prefix/JQL overrides are available through **g / Ctrl+G** or command-line flags; status mappings are unnecessary.

## Daily controls

- **Digits** automatically look up `<project-prefix>-<number>`, independent of assignment. No Enter required; Enter focuses details. Esc restores the view.
- **/** instantly filters the locally cached view, including descriptions. Words are matched locally; no server round trip is needed. Enter finishes editing the search; Esc clears. Searching does not switch tabs.
- **Tab**, **↑↓ / j k**, Page Up/Down and the mouse navigate. Internal links open in the client; Esc goes back. External links open the browser.
- **a** assigns: Me, Unassigned, known people, or another email/account ID. Display names are labels, not assignment identifiers. Cached combined-scope tickets also supply known people/statuses.
- **t** changes status, **e** edits summary/description/labels, **c** comments, **L** links tickets. Every update names **one ticket** and requires **y** confirmation; **n / Esc** cancels.
- **f**, **A / T**, or top dropdowns adjust assignee/status filters.
- **l** lists links/attachments, **O** opens Jira, **C** loads all comments, **r** refreshes, **!** shows the last full CLI error, **?** shows help, **q** quits.

Forms use Tab, Ctrl+U to clear, and Ctrl+S to review/save. The Markdown editor supports arrows, Home/End, Delete, Backspace and bracketed paste. Ctrl+S opens a confirmation preview.

Descriptions/comments render styled Markdown converted from Jira ADF. Supported Markdown edits convert back to ADF for acli. This is **not lossless for every Jira editor feature**: mentions, embedded media and unsupported extensions may change when rewriting descriptions. Review before confirming; use Jira web for those edits.

## Cache-first loading and background sync

- Startup immediately restores **one shared workspace snapshot** from the private account-scoped cache. Mine/Available/Backlog and text/status filters run locally; switching tabs never searches Jira.
- In parallel, acli paginates through **all scoped unfinished tickets**, including descriptions. A separate lightweight current-user key query establishes Mine membership without guessing from display names or parsing authentication tokens.
- Descriptions are usable as soon as the bulk fetch completes; cached full details are displayed immediately. Full details (comments, attachments, custom fields) warm in the background, prioritizing nearby rows, with at most **two background detail requests**. First-time full details can still require a CLI request, but no longer block the description.
- Every **60 seconds**, a background sync refreshes the shared index/membership and queries recently updated keys with an overlapping time window. Changed full details are revalidated; failed syncs keep the last local dataset. Dirty-detail markers survive restarts.
- Refreshes preserve the selected ticket/scroll where possible. Older responses cannot overwrite a newer workspace or a completed write. **r** requests an immediate refresh.
- All full details may persist on disk, but only **64** full tickets stay in memory. The shared summaries/descriptions remain available for instant filtering. No authenticated attachment downloading is introduced.

## Attachments and images

Attachments open in Jira web. The installed acli supports attachment list/delete, but **no authenticated download action**. The app never extracts tokens or silently downloads through Jira REST.

For downloaded images, place files at `<attachment-dir>/<ticket-key>/<original-filename>` and select the attachment, or press **i** to choose another local image. Set the directory with `--attachment-dir /path`; its default is account-scoped under the private cache. Paths reject traversal. Input is limited to 16 MiB / 12 million pixels.

Graphics are auto-detected with a Unicode half-block fallback. `--image-protocol kitty` forces Kitty graphics where supported; `--image-protocol halfblocks` is useful with multiplexers that do not pass graphics through. Esc closes previews. PNG/JPEG/GIF/WebP are supported as still images.

## Build and configuration

Requires Rust **1.90+**, a terminal, and an authenticated CLI.

```sh
make check
make install
jiraui
jiraui --demo
jiraui --project DEV
jiraui --project DEV --read-only --no-cache --check
```

Explicit scope flags are remembered for subsequent launches (except `--check`/demo). `--project` infers the matching prefix and replaces a previous scope unless `--prefix`/`--jql` are supplied. Automatic backend selection prefers the installed `acli`; override with `--backend acli|jira|demo` and optionally `--cli-bin /path/to/binary`. Authentication validity is still the CLI's responsibility. `/usr/bin/jira` is preferred over wrappers.

The acli workspace fetch uses **pagination**, not the old 500-row limit, so all scoped tickets are cached. `--limit` remains for legacy cache migration. The jira-cli adapter is still capped at 100 and explicitly marks a partial scope; prefer acli for complete datasets. Numeric lookup can open completed tickets even though the backlog lists exclude completed work.

`--read-only` blocks both update entry and confirmation. Writes are never automatically retried after failure/timeout; the outcome may be unknown. Reload before retrying. Development mutation tests use only synthetic DEV fixtures.

Settings live at `$XDG_CONFIG_HOME/jiraui/config.json`; ticket caches at `$XDG_CACHE_HOME/jiraui/rust/<account-hash>/`. CLI-reported identity/site determines the cache namespace; unknown identity disables persistent ticket caching. Files are 0600, directories 0700. `--no-cache` disables ticket persistence; demo saves neither settings nor caches. Restart after switching CLI account/site.

CLI jobs use argument arrays, asynchronous execution, concurrency limits, and timeouts. Rendering is event-driven; network updates happen independently of local navigation. Untrusted terminal controls are stripped and browser links must be ordinary HTTP(S) URLs. No direct Jira HTTP client or credential-file reading is used.

`src/backend.rs` delegates to CLIs; `src/workspace.rs` provides the shared cached dataset/local filtering; `src/config.rs` defines shared scope and the three filters; `src/app.rs` handles state/actions; `src/render.rs` provides the two-pane layout; `src/markdown.rs`, `src/data.rs`, and `src/input.rs` handle rich text, sanitization, and editing. The previous Go prototype remains in Git history.
