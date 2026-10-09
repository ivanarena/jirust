# jiraui

A lightweight **Rust + Ratatui** Jira terminal client. No browser engine, direct Jira HTTP client, or new authentication setup: all Jira reads and updates run through your existing **acli** or **jira-cli** installation.

```text
jiraui · scope     ^1 Board    ^2 Backlog    ^3 All    ^4 Mine
/ Search          Assignee: All ▾          Status: Any ▾
╭ Tickets ───────────────────╮╭ Ticket ───────────────────────╮
│ Key   Status  Assignee ... ││ Summary, reporter, assignee   │
│ DEV-101 ...                ││ Styled Markdown description  │
│ DEV-102 ...                ││ Links, attachments, comments │
╰───────────────────────────╯╰───────────────────────────────╯
```

## Build / run

Requires Rust **1.90+**, Linux (or macOS), and an authenticated Jira CLI.

```sh
make check
make install
jiraui --project DEV
jiraui --demo
jiraui --project DEV --read-only --check
```

Automatic backend selection prefers `acli`. Override with `--backend acli|jira|demo` and optionally `--cli-bin /path/to/binary`. `jira` prefers `/usr/bin/jira` to avoid wrappers. Credentials remain managed by the selected CLI; the app never reads tokens or credential files.

The old Go implementation is preserved in Git history; v0.2 uses Rust throughout.

## Simple workflow

- **Top views** replace the sidebar. Click a tab or press **Ctrl+1…9**. **Alt+number / F1…F9** are fallback bindings: legacy terminals cannot distinguish some Ctrl+number combinations from ordinary digits.
- **Type digits** to look up a ticket automatically. `3084` means `<configured-prefix>-3084`; prefix defaults to the project. No Enter is needed. Enter focuses details; `a` can assign the found ticket immediately. Esc restores the list.
- **/** starts debounced server text search across the configured project/JQL scope, not just loaded rows or “my work.” Loaded rows provide immediate local feedback while the server request is pending. Enter submits immediately. Esc clears search.
- **Tab** changes focus between Tickets and Ticket. **↑↓ / j k**, Page Up/Down and mouse-wheel scroll work. Click a row or a description link. Internal Jira links stay in the client; Esc returns to the previous ticket. External links open in your browser.
- **A / T**, or the top dropdowns, pick assignee/status filters. **f** accepts both filters together.
- **g / Ctrl+G** configures scope. **b** discovers board labels; this does not automatically infer the board filter. **Ctrl+S** saves the current query as another top view.
- **r** refreshes, **C** loads all comments, **!** shows the last full CLI error, **?** shows help, **q** quits.

“All” means all **unfinished** work in the configured scope; Mine adds current-user assignment. Numeric lookup can still open completed tickets. Default result limit is **500**, adjustable with `--limit 1..2000`. A potentially truncated list is explicitly marked. The `jira-cli` adapter is capped at 100; prefer `acli` for larger scopes.

## Assignment and updates

**a** opens an assignment picker:

1. Me (`@me`)
2. Unassigned
3. People already assigned to loaded tickets
4. Another email / account ID (jira-cli may use a login instead)

Display names are labels, not assignment identifiers. An independent confirmation names **one ticket**; **y** executes and **n / Esc** cancels. Jira permissions and workflow constraints are enforced by the CLI/server. User-directory discovery is not implemented; unknown people can be entered explicitly.

**t** changes status, **e** edits summary/description/labels, **c** adds a Markdown comment, **L** links tickets. Forms use Tab, Ctrl+U to clear, and Ctrl+S to review/save. The Markdown editor supports arrow keys, Home/End, Delete, Backspace and bracketed paste; Ctrl+S opens the confirmation preview.

Descriptions/comments display Markdown converted from Jira ADF, with headings, emphasis, lists, code, and clickable links. Editing converts supported Markdown back to ADF for acli. **This is not lossless for every Jira editor feature**: mentions, embedded media and unsupported extensions may change when rewriting a description. Review before confirming; use Jira web for those edits. All mutation tests use synthetic responses, not real Jira writes.

`--read-only` blocks update entry and confirmation. Failed or timed-out writes are not automatically retried: their outcome may be unknown. Reload before retrying.

## Board and Backlog: explicit scope, no sprints

The installed CLIs expose board names/IDs but **not the web board's saved filter and Kanban column/backlog configuration**. This app therefore does **not** claim to reproduce a selected web board automatically.

In **g** configure:

- Project, or the exact scope JQL / `filter = 12345` from the web board.
- The statuses Jira treats as **Backlog**, comma-separated.
- The status used when pulling a ticket onto **Board**.
- The status used when returning it to **Backlog**.

Board excludes the configured backlog statuses; Backlog includes them. Neither view makes sprint assumptions. Both refuse to run without a backlog-status mapping rather than guessing. **m** transitions one ticket to the configured entry status after confirmation. A status transition may not reproduce additional web-board behavior; verify your board's rules.

Example settings use synthetic data only:

```json
{
  "project": "DEV",
  "prefix": "DEV",
  "scope_jql": "project = DEV",
  "backlog_statuses": ["Refinement"],
  "board_entry_status": "To Do",
  "backlog_entry_status": "Refinement",
  "last_view": 2
}
```

## Attachments and terminal images

Attachments are clickable Jira web links. The installed acli offers attachment **list/delete**, but no authenticated download action. jiraui does not extract tokens or secretly download via Jira REST.

For an image already downloaded by you, place it at:

```text
<attachment-dir>/<ticket-key>/<original-filename>
```

Click its attachment link or use **l**. **i** previews another explicitly selected local image. `--attachment-dir /path` sets the directory; by default it lives under the active account's private cache namespace. Paths are validated against traversal. Preview input is limited to 16 MiB / 12 million pixels.

Graphics are automatically detected with a Unicode half-block fallback. Use `--image-protocol kitty` in a Kitty-graphics-capable terminal, or `--image-protocol halfblocks` when a multiplexer does not pass graphics through. Esc closes the preview. Supported image formats: PNG, JPEG, GIF, WebP. Animated images display a still frame; no video support.

## Local data and architecture

- Settings: `$XDG_CONFIG_HOME/jiraui/config.json` (default `~/.config/jiraui/config.json`). Existing Go project/saved queries are partially migrated.
- Cache: `$XDG_CACHE_HOME/jiraui/rust/<account-hash>/`. CLI-reported identity/site determines the namespace; no identity means no persistent ticket cache. Files are 0600, directories 0700. `--no-cache` disables ticket persistence. Demo does not save settings or ticket caches.
- Restart after switching CLI account/site. Automatic selection detects the executable, not whether its authentication is currently valid.
- CLI jobs run asynchronously with argument arrays, a concurrency limit, and timeouts. Input remains responsive while requests run. Rendering occurs on changes; lists cache/refresh immediately, details cache for two minutes, and in-memory full details are bounded to 64 tickets.
- Jira text is sanitized for terminal controls; browser links must be ordinary HTTP(S) URLs. Attachment graphics are local-only.
- `src/backend.rs`: CLI delegation. `src/data.rs`: models/sanitization/ADF. `src/config.rs`: scope and private persistence. `src/app.rs`: state/actions. `src/markdown.rs`: styled Markdown. `src/render.rs`: two-pane UI. `src/input.rs`: editor. `src/main.rs`: terminal and job loop.

Network/CLI latency remains the dominant request cost. Measure real startup/RSS instead of assuming a Rust rewrite makes Jira itself faster.
