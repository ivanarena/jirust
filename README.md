# jirust

A lightweight **Rust + Ratatui** Jira terminal client. Two panes, no browser engine. Ordinary operations use your existing **acli** or **jira-cli**. The optional Board/Backlog integration reuses acli authentication for Jira Software APIs.

```text
jirust · scope       ^1 Mine     ^2 Available     ^3 Board     ^4 Backlog
/ Search Mine        Assignee: Me ▾              Status: Any ▾
╭ Tickets ──────────────────╮╭ Ticket ───────────────────────╮
│ Key · Status · Assignee  ││ Description, reporter, links  │
│ Summary                 ││ Attachments and comments      │
╰─────────────────────────╯╰───────────────────────────────╯
```

## Four predefined views

One shared cached dataset feeds assignment views and separate membership tabs:

| Shortcut | View | Filter |
|---|---|---|
| Ctrl+1 | **Mine** | Assigned to the current CLI user |
| Ctrl+2 | **Available** | Unassigned in Board and Backlog together |
| Ctrl+3 | **Board** | Actual Board membership, including returned completed tickets |
| Ctrl+4 | **Backlog** | Actual Backlog membership |

Both Mine and Available include tickets from both sections, Board first, preserving Rank within each. Backlog uses the Jira Software backlog resource. Board uses the actual team-managed manual issue-list and configured completed-ticket visibility cutoff, never guessed status names. Text/status filters retain relative order; switching tabs starts at the top. Old tab indices/settings migrate without treating the old Backlog tab as Board. No sprint UI or status-mapping setup is required.

**←/→** switch tabs except while editing text. Click tabs or use **Alt+1…4 / F1…F4** if Ctrl+number is unavailable. Filters narrow the current view without switching tabs; Mine/Available still enforce their assignment presets and Board/Backlog their membership. Custom saved top views are not exposed.

Enable the approved API bridge with **`--board <id>`** (remembered in settings). It reads Jira's actual manual Board issue-list and completed-item cutoff, then hydrates Board/Backlog tickets in bulk with `ORDER BY Rank ASC`, isolating caches by board/account/scope. The public `/board/issue` endpoint alone is not membership: it also returns historical project work. Read-only web-board metadata supplies membership for team-managed `ISSUE_LIST` boards; unfamiliar strategies, truncated responses, or schema changes fail closed. The configured cutoff is evaluated against Jira's server-side `updated` field, not the local clock. Existing project/JQL restrictions still apply. Epics are included in their server-provided membership and Rank position, marked **Epic ·** with a purple key; `/ epic` finds them instantly by type. Higher-level containers and Sub-Tasks remain direct-lookup only. Epics are readable/editable tickets, but Board/Backlog movement is disabled for these project containers. API permissions or pagination failures keep the previous snapshot, never substitute status grouping. Without `--board`, the legacy unfinished CLI scope remains available in Backlog; Board membership/moves are unavailable. Web quick filters and completion-visibility policies are not assumed: verify live parity before calling any board an exact web-page replica.

## Move a ticket from the command line

```sh
jirust move DEV-123 --to board
jirust move DEV-123 --to backlog
```

The configured `JIRA_BOARD_ID` / remembered Board is used; override with `--board <id>`. Each command fetches verified live membership, checks that **one validated key** is in the current scope, names the source/destination and Board ID, and asks you to type the key to confirm. `--yes` is explicit single-ticket confirmation for noninteractive use. Membership/scope are revalidated after confirmation; already-at-destination is a no-op. No workflow transition is requested; Jira determines Rank. `--read-only` (including `JIRA_READ_ONLY=true`) blocks movement at entry and confirmation. Never retry uncertain writes automatically. In the TUI, **m** reviews the equivalent move.

## Daily controls

- **Digits** immediately open a locally known `<project-prefix>-<number>`, independent of assignment. Only unknown/out-of-scope numbers debounce a CLI lookup. No Enter required; Enter focuses details. Esc restores the view.
- **/** immediately filters a **prebuilt local text index**, including descriptions. No text-search debounce, description conversion, full-dataset cloning, or Jira round trip happens per keystroke. The selected result updates during the key event. Enter finishes editing; Esc clears. Searching does not switch tabs.
- **Tab**, **↑↓ / j k**, Page Up/Down and the mouse navigate. Internal links open in the client; Esc goes back. External links open the browser.
- **a** assigns: Me, Unassigned, known people, or another email/account ID. Display names are labels, not assignment identifiers. Cached combined-scope tickets also supply known people/statuses.
- **t** changes status, **e** edits summary/description/labels, **c** comments, **L** links tickets. Every update names **one ticket** and requires **y** confirmation; **n / Esc** cancels.
- **m** reviews moving one loaded ticket to the other membership section: Backlog → Board or Board → Backlog. **y** confirms; status is not changed. Jira determines the destination Rank; the subsequent refresh restores authoritative order. Requires verified API membership and `--board`. Read-only mode blocks entry and confirmation; uncertain writes are never retried automatically.
- **f / Filters** shows **both multi-select lists immediately, **Status first, Assignee second****, with current values checked. **Space / click toggles** a checkbox; ↑↓ / j k moves the cursor without changing checks; Tab or ←→ switches columns. **Enter / Ctrl+S / Apply** applies both columns together; Esc cancels all staged changes. Select several statuses and assignees: values within one column are **OR**, columns and text search are **AND**, further intersecting the current view. For example, `(Me OR Sam) AND (To Do OR In Progress) AND search words`. Each view remembers its own assignee/status checks and text search while Jirust runs; switching away does not apply them to another view, and returning restores them. Filters never switch the selected view or sort/reorder tickets. **All assignees / Any status** clears that column; Ctrl+U clears the active column; Ctrl+R / Reset stages clearing both (Apply commits). Background sync preserves every checked value and custom input. **A / T** or top dropdowns open the same multi-select popup in the corresponding column. **Other** adds one exact custom value without replacing existing checks; Enter adds it, Esc returns to the lists, Ctrl+S adds/applies. Commas are literal, not value separators. These are cached choices, not a complete Jira directory/workflow listing.
- **F6 / browsing x / Clear filters** clears only the **current view's** assignee/status filters and text/number search locally, retaining its Jira order and leaving other views untouched. Refresh preserves each view's filters; changing project/JQL scope resets all four: **0 shown · N in Backlog** means nothing matches, not that refresh deleted the Backlog. Empty lists explicitly explain this and offer F6; editor/dialog controls are untouched.
- **Status badges** use readable colored backgrounds in wide/narrow lists and ticket metadata, including selected rows. Every observed status name receives its **own distinct color**, even when several statuses share a Jira category. A collision-free palette is cached privately per account, consistent across views, metadata and filter choices, and stable when statuses are added/refreshed. Colors are visual only and never determine membership, order, or transitions.
- **F5 / Ctrl+R** pulls the complete shared workspace again (both Board and Backlog when configured), bypassing scoped ticket-detail caches. **r** does the same when browsing; F5/Ctrl+R also work during search. Selection, scroll, filters, and manual order are retained. Cached data stays visible; details revalidate with bounded background concurrency. Repeated presses coalesce; refresh waits for an existing sync and is blocked during writes. Inside Filters, Ctrl+R still resets pending filters; editor controls are not intercepted.
- **l** lists links/attachments, **O** opens Jira, **C** loads all comments, **!** shows the last full CLI error, **?** shows help, **q** quits.

Forms use Tab, Ctrl+U to clear, and Ctrl+S to review/save. The Markdown editor supports arrows, Home/End, Delete, Backspace and bracketed paste. Ctrl+S opens a confirmation preview.

Descriptions/comments render styled Markdown converted from Jira ADF. Tables use aligned borders, styled headers, per-cell wrapping, Unicode-aware widths, and clickable links. Narrow panes fall back to labeled cells; multiline ADF table cells retain their content. Supported Markdown edits convert back to ADF for acli. This is **not lossless for every Jira editor feature**: mentions, embedded media and unsupported extensions may change when rewriting descriptions. Review before confirming; use Jira web for those edits.

## Cache-first loading and background sync

- Startup immediately restores **one shared workspace snapshot** from the private account-scoped cache. Mine/Available/Board/Backlog and text/status filters run locally; switching tabs never searches Jira.
- With `--board`, API membership pages and lightweight CLI Mine/change queries run concurrently. Otherwise acli paginates unfinished scope. A server `currentUser()` query establishes Mine, never display-name guesses.
- Descriptions are usable as soon as the bulk fetch completes; cached full details are displayed immediately. Full details (comments, attachments, custom fields) warm in the background, prioritizing nearby rows, with at most **two background detail requests**. First-time full details can still require a CLI request, but no longer block the description.
- Input bursts are handled before background messages and painted together, so background warming cannot continuously preempt typing. Cache JSON is decoded/encoded directly, avoiding intermediate copies. `cargo run --release --example search_bench` benchmarks 1,000 synthetic tickets with 32 ADF paragraphs each (no account data or Jira operations).
- Every **60 seconds**, a background sync refreshes the shared index/membership and queries recently updated keys with an overlapping time window. Changed full details are revalidated; failed syncs keep the last local dataset. Dirty-detail markers survive restarts.
- Refreshes preserve the selected ticket/scroll where possible. Older responses cannot overwrite a newer workspace or a completed write. **F5 / Ctrl+R / r** requests a full refresh, including revalidation of scoped full details even when the change query would find nothing.
- All full details may persist on disk, but only **64** full tickets stay in memory. The shared summaries/descriptions remain available for instant filtering. No authenticated attachment downloading is introduced.

## Attachments and images

Attachments open in Jira web. The installed acli supports attachment list/delete, but **no authenticated download action**. The optional credential reuse is restricted to Board membership/moves; attachments are never silently downloaded through Jira REST.

For downloaded images, place files at `<attachment-dir>/<ticket-key>/<original-filename>` and select the attachment, or press **i** to choose another local image. Set the directory with `--attachment-dir /path`; its default is account-scoped under the private cache. Paths reject traversal. Input is limited to 16 MiB / 12 million pixels.

Images render directly in the terminal **as local previews**; automatic inline Jira media display/authenticated downloads are not implemented. Graphics are auto-detected with a Unicode half-block fallback. `--image-protocol kitty` forces Kitty graphics where supported; `--image-protocol halfblocks` is useful with multiplexers that do not pass graphics through. Esc closes previews. PNG/JPEG/GIF/WebP are supported as still images.

## Build and configuration

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

The acli workspace fetch uses **pagination**, not the old 500-row limit, so all scoped tickets are cached. `--limit` remains for legacy cache migration. The jira-cli adapter is still capped at 100 and explicitly marks a partial scope; prefer acli for complete datasets. Numeric lookup can open completed tickets; API Board snapshots retain completed tickets returned by Jira.

`--read-only` blocks both update entry and confirmation. Writes are never automatically retried after failure/timeout; the outcome may be unknown. Reload before retrying. Development mutation tests use only synthetic DEV fixtures.

Platform-native locations:

| Platform | Settings / private env directory | Ticket cache directory |
|---|---|---|
| Linux | `${XDG_CONFIG_HOME:-~/.config}/jirust` | `${XDG_CACHE_HOME:-~/.cache}/jirust/rust/<account-hash>` |
| macOS | `~/Library/Application Support/jirust` | `~/Library/Caches/jirust/rust/<account-hash>` |

Settings are `config.json`; the optional private env is `.env`. CLI-reported identity/site determines the cache namespace; unknown identity disables persistent ticket caching. Files are 0600, directories 0700. `--no-cache` disables ticket persistence; demo saves neither settings nor caches. Restart after switching CLI account/site.

When upgrading from the old app name, **quit the old process first**, then start `jirust`. On first use, legacy config/cache directories are renamed to the new directories, preserving the private environment file, settings, account namespaces and attachments without rewriting their contents. An existing new directory is never overwritten. Demo does not migrate personal configuration.

### Local environment configuration

Copy the synthetic `.env.example` to **`.env` in the project directory**, or to **`.env` in the platform-specific config directory above**, and set permissions to **0600**. Put the company HTTPS URL, email, API token without scopes, project/prefix, Board ID and optional settings there. The real file is plaintext: protect it and never commit/share it. The tracked template contains no credentials. Alternatively use a private `./.env`, or select an explicit file with `--env-file /path/to/.env`.

Precedence: explicit CLI flags → exported environment variables → selected dotenv file → saved/default settings. File discovery: explicit `--env-file`, otherwise `./.env`, otherwise the user config dotenv file. File values are not exported into the process or child environments. Demo ignores local/company configuration entirely. A new explicit project does not inherit another project's environment Board/prefix/JQL.

```sh
jirust auth                            # token piped privately to acli stdin
jirust --read-only --no-cache --check  # validate without changing tickets
jirust                                # start the TUI
```

`JIRA_URL`, `JIRA_EMAIL` and `JIRA_API_TOKEN` must be supplied together. Only HTTPS Atlassian Cloud origins are accepted; credential-bearing URLs, redirects and arbitrary hosts are rejected. The Board bridge uses the local token directly; ordinary operations still use acli authentication. `auth` explicitly updates acli's own keyring/profile from that token; startup never silently switches CLI accounts. A mismatched site/email fails closed and asks you to authenticate explicitly. Restart after editing authentication variables.

Other variables: `JIRA_PROJECT`, `JIRA_PREFIX`, `JIRA_BOARD_ID`, `JIRA_JQL`, `JIRA_BACKEND`, `JIRA_CLI_BIN`, `JIRA_READ_ONLY`, `JIRA_NO_CACHE`, `JIRA_LIMIT`, `JIRA_ATTACHMENT_DIR`, `JIRA_IMAGE_PROTOCOL`. Boolean values are `true` / `false` (also `1` / `0`). Tokens are never placed in command arguments, debug output, settings JSON or caches; dotenv/auth errors redact values. Do not combine `--check` with a subcommand.

### macOS setup (Intel or Apple Silicon)

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

In the private file, replace the synthetic URL/email/token/project/Board placeholders with your own values. Use an API token **without scopes**. Leave `JIRA_CLI_BIN` empty to find Homebrew's `acli` on PATH; never copy a Linux-specific `/usr/bin/acli` setting to a Mac. Then:

```sh
jirust --env-file "$PWD/.env" auth
jirust --env-file "$PWD/.env" --read-only --no-cache --check
jirust --env-file "$PWD/.env"
# Later, one confirmed membership move:
jirust --env-file "$PWD/.env" move DEV-123 --to board
```

Replace `DEV-123` with the actual one-ticket key. The explicit file path works from any directory when changed to an absolute path. Alternatively keep the file at `~/Library/Application Support/jirust/.env` for automatic discovery. Add `~/.local/bin` to your shell's PATH permanently if desired. Browser links use macOS `open`; image previews have a Unicode fallback. No GNU `install -D` or Linux desktop integration is required.

**Mac Board access requires a local API token.** The keyring reuse fallback below is Linux-only; Jirust does not read macOS Keychain itself. `jirust auth` delegates secure CLI credential storage to acli. Metadata can be read from acli's standard `~/.config/acli` or native platform config directory; conflicting active profiles fail closed instead of guessing an account.

CI runs build, lint, unit tests and a synthetic read-only demo on Linux and macOS. These tests require no credentials and make no real Jira writes. A successful build/demo does **not** establish live Jira or terminal-graphics parity on every Mac; verify your own account with the read-only check above.

### Board API authentication without a local token

Linux `secret-tool` must be installed when using the keyring fallback. The bridge reads only the active acli Jira profile's keyring entry; credentials are kept in memory. Both site OAuth and global OAuth are supported; global OAuth uses its own keyring entry and requires matching active global/Jira identities. Some OAuth grants work for ordinary issues but lack Jira Software API scopes (HTTP 401 `scope does not match`). A successful browser/global OAuth login alone does not grant missing scopes. The app cannot grant these permissions. Use a supported acli API-token login **without scopes**, supplying the token privately through stdin: `acli jira auth login --site mysite.atlassian.net --email you@example.com --token`. Do not put tokens in command arguments, this repository, or chat. Restart afterward, then validate read-only with `jirust --board 17 --read-only --check`. Live membership/parity must be verified for each board after authorization succeeds.

CLI jobs use argument arrays, asynchronous execution, concurrency limits, and timeouts. Rendering is event-driven; network updates happen independently of local navigation. Untrusted terminal controls are stripped and browser links must be ordinary HTTP(S) URLs. Only the explicit `--board` API bridge uses direct HTTP with a private environment token or current acli keyring credentials. It pins HTTPS origins, disables redirects, bounds concurrency/response sizes, and never logs tokens or writes them into app settings/caches. The only user-approved plaintext credential storage is the private local environment file. Account/site/auth changes require restart. OAuth login and refresh remain delegated to acli.

`src/environment.rs` loads private local configuration; `src/movement.rs` validates confirmed CLI moves; `src/backend.rs` delegates ordinary operations to CLIs; `src/board_api.rs` implements the opt-in membership bridge; `src/workspace.rs` provides the shared cached dataset/local filtering; `src/config.rs` defines scope and the four views; `src/app.rs` handles state/actions; `src/render.rs` provides the two-pane layout; `src/markdown.rs`, `src/data.rs`, and `src/input.rs` handle rich text, sanitization, and editing. The previous Go prototype remains in Git history.
