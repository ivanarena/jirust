# jirust

An **experimental**, lightweight **Rust + Ratatui** Jira terminal client. Two panes, no browser engine. Ordinary operations use your existing **acli** or **jira-cli**. The optional Board/Backlog integration and read-only raster attachment downloads/field metadata lookup reuse the pinned acli account's authentication.

See the [support matrix](docs/SUPPORT.md), [security policy](SECURITY.md) and [release checklist](docs/RELEASING.md). Native macOS CI is not live Jira/terminal validation; exact membership is not supported for every Jira board strategy.

```text
jirust · scope       ^1 Mine     ^2 Available     ^3 Board     ^4 Backlog
/ Search Mine        Status: Any ▾               Assignee: Me ▾
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

By default, Mine and Available include tickets from both sections, Board first, preserving Rank within each. Backlog uses the Jira Software backlog resource. Board uses the actual team-managed manual issue-list and configured completed-ticket visibility cutoff, never guessed status names. Text/status filters retain relative order; switching tabs starts at the top. Old tab indices/settings migrate without treating the old Backlog tab as Board. No sprint UI or status-mapping setup is required.

**←/→** switch tabs except while editing text. Click tabs or use **Alt+1…4 / F1…F4** if Ctrl+number is unavailable. Filters narrow the current view without switching tabs; Mine/Available still enforce their assignment presets and Board/Backlog their membership. Custom saved top views are not exposed.

Enable the approved API bridge with **`--board <id>`** (remembered in settings). It reads Jira's actual manual Board issue-list and completed-item cutoff, then hydrates Board/Backlog tickets in bulk with `ORDER BY Rank ASC`, isolating caches by board/account/scope. The public `/board/issue` endpoint alone is not membership: it also returns historical project work. Read-only web-board metadata supplies membership for team-managed `ISSUE_LIST` boards; unfamiliar strategies, truncated responses, or schema changes fail closed. The configured cutoff is evaluated against Jira's server-side `updated` field, not the local clock. Existing project/JQL restrictions still apply. Epics are included in their server-provided membership and Rank position, marked **Epic ·**, with ordinary row/title styling; `/ epic` finds them instantly by type. A ticket's **assigned parent epic**, when Jira explicitly identifies its parent as an Epic (type/hierarchy level), appears as a **distinct, per-epic colored label before its summary** in wide/narrow lists and above its title in details. Only that label is colored, not the ticket row or epic tickets; status badge colors remain unchanged. Each epic key receives its own collision-free color, chosen for perceptual separation and contrast-safe black/white text, privately cached per account and stable across views, list/details, additions, renames and restarts (not a repeating fixed-palette hash). Long labels are shortened by Unicode cell width to leave room for the ticket title. Labels use the parent summary (or key if unnamed); the detail label opens that epic. Unknown/non-epic parents, linked issues, and missing relationships produce no label: never guess custom-field IDs or relationships. Parent data comes from the existing approved Board/Backlog hydration and CLI full details (acli search cannot project parent); fresh sync/details update and clear cached labels. Labels alone do not group/reorder tickets or add epic members (optional Epic sorting is described below), and are searchable in the local index. Higher-level containers and Sub-Tasks remain direct-lookup only. Epics are readable/editable tickets, but Board/Backlog movement is disabled for these project containers. API permissions or pagination failures keep the previous snapshot, never substitute status grouping. Without `--board`, the legacy unfinished CLI scope remains available in Backlog; Board membership/moves are unavailable. Web quick filters and completion-visibility policies are not assumed: verify live parity before calling any board an exact web-page replica.

## Optional Epic sorting

**E** (Shift+e) or the clickable **E Sort: Rank / Sort: Epic** footer button toggles the current view's local order. **Rank is the default**. Epic sorting groups tickets by their explicitly assigned parent epic, alphabetically by epic name (key breaks name ties), and puts tickets with no parent epic last. Jira Rank is retained within each epic/no-epic group. Mine/Available still keep **Board before Backlog**, with epic groups within each section. Epic tickets themselves have no inferred parent group or special styling.

Each view remembers its own sort while Jirust runs. Refreshes, successful writes, search and filter changes retain it; changing scope or restarting resets it to Rank. Selection and detail reading position are retained when toggling. Sorting is cached on data changes, not rerun for every search keystroke. It never changes Jira Rank/membership, writes to Jira, adds rows or groups linked/non-epic parents. E does not intercept editor/search text; lower-case e still edits tickets.

## Move a ticket from the command line

```sh
jirust move DEV-123 --to board
jirust move DEV-123 --to backlog
```

The configured `JIRA_BOARD_ID` / remembered Board is used; override with `--board <id>`. Each command fetches verified live membership, checks that **one validated key** is in the current scope, names the source/destination and Board ID, and asks you to type the key to confirm. `--yes` is explicit single-ticket confirmation for noninteractive use. Membership/scope are revalidated after confirmation; already-at-destination is a no-op. No workflow transition is requested; Jira determines Rank. `--read-only` (including `JIRA_READ_ONLY=true`) blocks movement at entry and confirmation. Never retry uncertain writes automatically. In the TUI, **m** reviews the equivalent move.

## Daily controls

- **Digits** immediately open a locally known `<project-prefix>-<number>`, independent of assignment. Only unknown/out-of-scope numbers debounce a CLI lookup. No Enter required; Enter focuses details. Esc restores the view.
- **/** immediately filters a **prebuilt local text index**, including descriptions. No text-search debounce, description conversion, full-dataset cloning, or Jira round trip happens per keystroke. The selected result updates during the key event. Enter finishes editing; Esc clears. Searching does not switch tabs.
- **Tab**, **↑↓ / j k**, Page Up/Down and the mouse navigate. **Click a ticket row to select it and automatically copy its Jira browse link** (not its key or an API URL). Re-clicking copies again; arrows, scrolling, tabs, headers/borders and dialogs never copy. Works in read-only mode and never modifies Jira. Native clipboard helpers use stdin only: macOS `pbcopy`, Wayland `wl-copy`, or X11 `xclip`/`xsel`; missing/failed helpers and SSH use a generated terminal **OSC52** request (terminal/multiplexer support must be enabled). Copies are bounded, asynchronous, serialized and coalesced so rapid clicks end with the newest link. Clipboard contents are never read. **`--no-copy-on-click`** keeps ordinary row selection without clipboard writes. Internal detail links still open in the client; Esc goes back. External detail links still open the browser.
- **Unassigned person fields are blank** in lists, ticket details and the single-value assignee filter display, without changing filter/membership semantics. **Unassigned** remains an explicit assignment/filter choice so it can still be selected.
- **a** assigns: Me, Unassigned, known people, or another email/account ID. Display names are labels, not assignment identifiers. Cached combined-scope tickets also supply known people/statuses.
- **s** changes status (**t** remains a compatibility alias), **e** edits summary/description/labels, **c** comments, **L** links tickets. Every update names **one ticket** and requires **y** confirmation; **n / Esc** cancels.
- **m** reviews moving one loaded ticket to the other membership section: Backlog → Board or Board → Backlog. **y** confirms; status is not changed. Jira determines the destination Rank; the subsequent refresh reloads authoritative Rank and reapplies the selected local sort. Requires verified API membership and `--board`; TUI and CLI moves both re-fetch scope/membership after confirmation and skip an already-completed move. Read-only mode blocks entry and confirmation; uncertain writes are never retried automatically.
- **f / Filters** shows **both multi-select lists immediately, **Status first, Assignee second****, with current values checked. **Space / click toggles** a checkbox; ↑↓ / j k moves the cursor without changing checks; Tab or ←→ switches columns. **Enter / Ctrl+S / Apply** applies both columns together; Esc cancels all staged changes. Select several statuses and assignees: values within one column are **OR**, columns and text search are **AND**, further intersecting the current view. For example, `(Me OR Sam) AND (To Do OR In Progress) AND search words`. Each view remembers its own assignee/status checks and text search while Jirust runs; switching away does not apply them to another view, and returning restores them. Filters never switch the selected view or sort/reorder tickets. **All assignees / Any status** clears that column; Ctrl+U clears the active column; Ctrl+R / Reset stages clearing both (Apply commits). Background sync preserves every checked value and custom input. **A / S** or top dropdowns open the same multi-select popup in the corresponding column. **Other** adds one exact custom value without replacing existing checks; Enter adds it, Esc returns to the lists, Ctrl+S adds/applies. Commas are literal, not value separators. These are cached choices, not a complete Jira directory/workflow listing.
- **F6 / browsing x / Clear filters** clears only the **current view's** assignee/status filters and text/number search locally, retaining its selected Rank/Epic order and leaving other views untouched. Refresh preserves each view's filters; changing project/JQL scope resets all four: **0 shown · N in Backlog** means nothing matches, not that refresh deleted the Backlog. Empty lists explicitly explain this and offer F6; editor/dialog controls are untouched.
- **Status badges** use readable colored backgrounds in wide/narrow lists and ticket metadata, including selected rows. **Done uses a solid green badge; Discarded uses solid red**, regardless of when those names first appear. Other names receive saturated backgrounds chosen for the greatest perceptual separation from already assigned colors, rather than similar pale tints. Black/white text is selected for at least 4.5:1 contrast. Every observed status name receives its own distinct background, even when statuses share a Jira category; green/red are reserved for those two exact names, never inferred from category. The new v2 palette replaces older pale assignments once, then remains privately cached per account, consistent across views, metadata and filter choices, and stable when statuses are added/refreshed. **Done → ✓**, **Discarded → ✕** in list badges/metadata; filter choices retain the full workflow names beside the icons, and confirmation/actions use the original names. Colors/icons are visual only and never determine membership, order, or transitions.
- **F5 / Ctrl+R** pulls the complete shared workspace again (both Board and Backlog when configured), bypassing scoped ticket-detail caches. **r** does the same when browsing; F5/Ctrl+R also work during search. Selection, scroll, filters, authoritative Rank and the selected local sort are retained. Cached data stays visible; details revalidate with bounded background concurrency. Repeated presses coalesce; refresh waits for an existing sync and is blocked during writes. Inside Filters, Ctrl+R still resets pending filters; editor controls are not intercepted.
- **l** lists links/attachments, **O** opens Jira, **C** loads all comments, **!** shows the last safe error details (raw CLI diagnostics are suppressed), **?** shows help, **q** quits.

**Every successful confirmed change** (assignment, status, edits, comments, links or Board/Backlog moves) automatically runs a **full shared-workspace refresh for all four views**, independent of the active view or filters, and revalidates cached details. Each view's filters/search remain intact; selection/scroll are kept where the ticket still matches (otherwise the view selects a remaining match). Reads started before the change cannot overwrite the refreshed state. Failed/uncertain writes keep the error visible and never retry the write automatically; a failed refresh retains cached tickets and does not repeat the completed write.

Forms use Tab, Ctrl+U to clear, and Ctrl+S to review/save. The Markdown editor supports arrows, Home/End, Delete, Backspace and bracketed paste. Ctrl+S opens a confirmation preview.

Descriptions/comments render styled Markdown converted from Jira ADF. Tables use aligned borders, styled headers, per-cell wrapping, Unicode-aware widths, and clickable links. Narrow panes fall back to labeled cells; multiline ADF table cells retain their content. Supported Markdown edits convert back to ADF for acli. This is **not lossless for every Jira editor feature**: mentions, embedded media and unsupported extensions may change when rewriting descriptions. Review before confirming; use Jira web for those edits.

## Story points

The wide list includes an **SP** column; narrow rows show **SP:value** before the assignee. Ticket metadata includes **Story points**. Zero and fractional estimates remain visible; unset, unavailable or ambiguous estimates are **blank**, never shown as zero or a dash. Narrow rows omit the SP label when blank. Epics show only their own estimate, never a guessed sum of child tickets.

The approved read-only `GET /rest/api/3/field` lookup identifies numeric story-point fields by Jira's names/schema, not hardcoded custom-field IDs. It is account/site pinned, redirect-free, limited to 2 MiB/10,000 definitions/32 candidates, and runs once per session. Bulk Board/Backlog hydration requests those fields in the existing authorized membership fetch; ordinary full-ticket reads still use the CLI. Without the Board bridge, estimates become available as full details load (or from server name metadata with jira-cli). Populated conflicting candidates, malformed values or a failed lookup leave the field blank instead of guessing; restart after fixing field permissions to retry metadata discovery. No story-point editing is introduced.

## Cache-first loading and background sync

- Startup immediately restores **one shared workspace snapshot** from the private account-scoped cache. Mine/Available/Board/Backlog and text/status filters run locally; switching tabs never searches Jira.
- With `--board`, API membership pages and lightweight CLI Mine/change queries run concurrently. Otherwise acli paginates unfinished scope. A server `currentUser()` query establishes Mine, never display-name guesses.
- Descriptions are usable as soon as the bulk fetch completes; cached full details are displayed immediately. Full details (comments, attachments, custom fields) warm in the background, prioritizing nearby rows, with at most **two background detail requests**. First-time full details can still require a CLI request, but no longer block the description.
- Input bursts are handled before background messages and painted together, so background warming cannot continuously preempt typing. Cache JSON is decoded/encoded directly, avoiding intermediate copies. `cargo run --release --example search_bench` benchmarks 1,000 synthetic tickets with 32 ADF paragraphs each (no account data or Jira operations).
- Every **60 seconds**, a background sync refreshes the shared index/membership and queries recently updated keys with an overlapping time window. Changed full details are revalidated; failed syncs keep the last local dataset. Dirty-detail markers survive restarts.
- Refreshes preserve the selected ticket/scroll where possible. Older responses cannot overwrite a newer workspace or a completed write. **F5 / Ctrl+R / r** requests a full refresh, including revalidation of scoped full details even when the change query would find nothing.
- CLI output is capped at **32 MiB stdout / 256 KiB stderr**; stderr is discarded, not shown through error details. JSON/failed-write receipt errors suppress raw content too. Workspaces are capped at **64 MiB encoded data / 20,000 tickets**, not silently truncated; failures retain the previous useful snapshot. Managed JSON cache reads/serialization are capped at **64 MiB per entry**, with **256 MiB / 4,096 entries per account namespace**. Quota/lock failures leave existing cache entries intact; persistence is best-effort, not a global cross-account disk quota. These are encoded-data limits, not an exact RSS cap.
- All full details may persist on disk within those limits, but only **64** full tickets stay in memory. The shared summaries/descriptions remain available for instant filtering. Image downloads/decoding are separately bounded, cancellable and off the render/input thread.

## Attachments and images

**PNG/JPEG/GIF/WebP attachments render inline** in an **Images** section after the description, as still images. **i** jumps there (and retries failed images); **l** or a clickable image caption opens a larger **tab-wide preview**. **I** chooses a local file; i also does this when the current document has no inline image section. Esc closes a preview. ADF media UUIDs are not reliably attachment IDs, so images appear in this attachment section rather than guessed original ADF positions. External media URLs, SVG and non-raster attachments are not fetched/rendered automatically; non-raster links open Jira web.

The installed acli has no authenticated download action, so the explicitly approved read-only bridge verifies the numeric attachment ID on the selected ticket through acli, then uses the pinned HTTPS Cloud attachment-content endpoint with `redirect=false`. It does **not** follow redirects, forward credentials to CDN hosts, or log signed URLs/tokens. Redirected/unsupported responses show an inline error; download through Jira web manually instead. Ordinary issue operations still use the CLI, and OAuth refresh remains its responsibility. This feature requires a matching authenticated acli cloud profile, but not a Board ID. Read-only mode permits these GETs; no attachment upload/delete is implemented.

Images now **fill the ticket pane's width**, preserve their pixel aspect ratio and scroll vertically in slices when taller than the viewport; they are no longer ten-row thumbnails. Smaller originals are scaled up too. Scaled rasters are additionally capped at 4 million pixels/4,096 rows (pathological tall images show an error rather than allocate unbounded buffers); long/wide Kitty images use the universal renderer to avoid its finite Unicode-placeholder table. Any intersecting image block can load automatically, at most **two requests/decoders** at once and **eight inline attachments** per ticket; other images remain selectable with l. Ticket changes/resizes cancel obsolete requests and reject late results; offscreen protocols leave memory. Bodies are capped at **16 MiB**, **12 million pixels**, **64 MiB decoder allocation** and a 45-second HTTP timeout. ID-keyed account-private caches use 0600 files/0700 directories and a **128 MiB** quota; duplicate filenames cannot collide. `--no-cache` avoids image persistence too. **`--no-inline-images`** disables automatic inline loading, retaining explicitly selected/local previews.

Graphics are auto-detected with a Unicode half-block fallback. `--image-protocol kitty` forces Kitty graphics where supported; `--image-protocol halfblocks` works with multiplexers that do not pass graphics through. The image caption identifies the chosen protocol. Live native graphics on every macOS terminal are not guaranteed.

For an explicit no-terminal, no-cache read-only download/decoder diagnostic: `cargo run --release --example image_check -- DEV-123` (replace the synthetic key with your selected live ticket).

Existing manual images still work at `<attachment-dir>/<ticket-key>/<original-filename>`; select the attachment or use I for another local file. `--attachment-dir /path` overrides the account-private default. Manual attachment paths reject traversal and use the same bounded raster decoder.

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

The acli workspace fetch uses **pagination**, not the old 500-row limit, so the complete scoped dataset is fetched within the documented resource budgets. Oversized scopes fail explicitly; they are never presented as complete truncated membership. `--limit` remains for legacy cache migration. The jira-cli adapter is still capped at 100 and explicitly marks a partial scope; prefer acli for complete datasets. Numeric lookup can open completed tickets; API Board snapshots retain completed tickets returned by Jira.

`--read-only` blocks both update entry and confirmation. Writes are never automatically retried after failure/timeout; the outcome may be unknown. Reload before retrying. Development mutation tests use only synthetic DEV fixtures.

Platform-native locations:

| Platform | Settings / private env directory | Ticket cache directory |
|---|---|---|
| Linux | `${XDG_CONFIG_HOME:-~/.config}/jirust` | `${XDG_CACHE_HOME:-~/.cache}/jirust/rust/<account-hash>` |
| macOS | `~/Library/Application Support/jirust` | `~/Library/Caches/jirust/rust/<account-hash>` |

Settings are `config.json`; the optional private env is `.env`. CLI-reported identity/site determines the cache namespace; unknown identity disables persistent ticket caching. Files are 0600, directories 0700. Cache/config opens validate Unix ownership, private permissions, regular files and no-follow leaf paths; symlinks/hardlinks are rejected rather than trusted. Atomic writes and nonblocking per-directory locks protect cache updates/quotas across client processes. `--no-cache` disables ticket persistence; demo saves neither settings nor caches. Restart after switching CLI account/site.

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

**Mac Board/image/field HTTP access requires a local API token.** The keyring reuse fallback below is Linux-only; Jirust does not read macOS Keychain itself. `jirust auth` delegates secure CLI credential storage to acli. Metadata can be read from acli's standard `~/.config/acli` or native platform config directory; conflicting active profiles fail closed instead of guessing an account.

CI runs build, lint, unit tests and a synthetic read-only demo on Linux and macOS. These tests require no credentials and make no real Jira writes. A successful build/demo does **not** establish live Jira or terminal-graphics parity on every Mac; verify your own account with the read-only check above.

### Board API authentication without a local token

Linux `secret-tool` must be installed when using the keyring fallback. The bridge reads only the active acli Jira profile's keyring entry; credentials are kept in memory. Both site OAuth and global OAuth are supported; global OAuth uses its own keyring entry and requires matching active global/Jira identities. Some OAuth grants work for ordinary issues but lack Jira Software API scopes (HTTP 401 `scope does not match`). A successful browser/global OAuth login alone does not grant missing scopes. The app cannot grant these permissions. Use a supported acli API-token login **without scopes**, supplying the token privately through stdin: `acli jira auth login --site mysite.atlassian.net --email you@example.com --token`. Do not put tokens in command arguments, this repository, or chat. Restart afterward, then validate read-only with `jirust --board 17 --read-only --check`. Live membership/parity must be verified for each board after authorization succeeds.

CLI jobs use argument arrays, asynchronous execution, concurrency limits, and timeouts. Rendering is event-driven; network updates happen independently of local navigation. Untrusted terminal controls are stripped and browser links must be ordinary HTTP(S) URLs. Only the approved Board bridge, read-only verified raster attachment downloader, and story-point field metadata lookup use direct HTTP with a private environment token or current acli keyring credentials. It pins HTTPS origins, disables redirects, bounds concurrency/response sizes, and never logs tokens or writes them into app settings/caches. The only user-approved plaintext credential storage is the private local environment file. Account/site/auth changes require restart. OAuth login and refresh remain delegated to acli.

`src/environment.rs` loads private local configuration; `src/movement.rs` validates confirmed CLI moves; `src/backend.rs` delegates ordinary operations to CLIs; `src/board_api.rs` implements the membership/image HTTP bridges; `src/images.rs` provides bounded full-width sliced rasters and private quota-limited caching; `src/estimates.rs` identifies authoritative story-point fields; `src/workspace.rs` provides the shared cached dataset/local filtering; `src/config.rs` defines scope and the four views; `src/app.rs` handles state/actions; `src/render.rs` provides the two-pane layout; `src/markdown.rs`, `src/data.rs`, and `src/input.rs` handle rich text, sanitization, and editing. The previous Go prototype remains in Git history.
