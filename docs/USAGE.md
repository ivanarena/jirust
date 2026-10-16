# Usage

[Quick start](../README.md) · [Configuration](CONFIGURATION.md) · [Support limits](SUPPORT.md)

## Views and ordering

| View | Contents |
|---|---|
| Mine | Current CLI user's assigned tickets across Board and Backlog |
| Available | Unassigned tickets across both sections |
| Board | Verified Board membership, including returned completed tickets |
| Backlog | Verified Backlog membership |

Use Ctrl+1…4, Alt+1…4, F1…F4, tabs or browsing ←/→. All views share one cached dataset. Mine/Available keep Board before Backlog, preserving Jira Rank within each section.

Exact membership requires a supported Board ID (`JIRA_BOARD_ID` / `--board`). The bridge recognizes team-managed manual `ISSUE_LIST` metadata and completed-ticket visibility rules; it never guesses membership from statuses/sprints. Unsupported, overlapping or truncated responses fail closed and retain useful cached data. Without a Board, the unfinished CLI scope remains browsable, but exact Board membership/moves are unavailable. The legacy jira-cli adapter is partial/capped at 100; acli paginates within resource limits.

**E / Shift+e** toggles the current view's Rank/Epic sort. Epic order groups explicit parent epics alphabetically by name/key, no parent last, with stable Rank within groups and Board still before Backlog. Epic tickets have no inferred parent group. Sort is local, retained through refresh/filter changes, and reset on scope change/restart; selection/reading position are preserved where possible. Lowercase e still edits.

## Search and filters

- **/** searches the local index, including descriptions, type and explicit parent-epic labels. It does not fetch Jira or rebuild documents/sort on every keystroke.
- **Digits** search `<prefix>-<number>` immediately in cached data; unknown numbers debounce a CLI lookup. Enter focuses details; Esc restores the view.
- **f**, **A** or **S** opens Status/Assignee filters. Space/click toggles; Tab/←→ switches lists; Enter/Ctrl+S applies; Esc cancels. Ctrl+U clears a column; Ctrl+R stages resetting both. Other adds an exact custom value; commas are literal.
- Values are **OR within a field, AND across fields/text**. Mine/Available assignment presets and Board/Backlog membership still apply. Each view retains independent runtime filters/search.
- **Double-click** a rendered status, assignee or epic label in a row/detail metadata to replace only that field's current-view filter. Other filters/text/sort remain. Assignees use exact identities, statuses full names, epics explicit keys—not equal display labels. Description words do not become filter gestures.
- An Epic filter includes that epic and explicitly parented tickets already in scope, never guessed/linked members. **v** clears only Epic; **F6 / browsing x** clears all current-view filters/search while preserving sort. Changing scope resets all views' runtime filters.

An empty filtered view does not imply deleted tickets: compare shown/scope counts and try F6. Filters/search/sort never write Jira.

## Navigation, links and clipboard

Tab switches panes; arrows/j/k, Page Up/Down and mouse navigate. **l** lists links/attachments, **O** opens Jira, **C** loads comments, **!** shows the last safe error, **?** opens help, **q** quits.

A row click selects and copies its full browse link. The second field click filters without another copy. Keyboard/scroll/tabs/dialogs never copy. `--no-copy-on-click` disables these clipboard writes. Native helpers use stdin: macOS pbcopy, Wayland wl-copy, or X11 xclip/xsel; SSH/missing helpers use terminal OSC52, which requires host support. The application never reads clipboard contents.

Internal ticket links open in the client; Esc goes back. Single detail-parent-epic links wait 350 ms to distinguish navigation from double-click filtering; other detail links open immediately. External links open the browser. Generated links honor the optional browser-only base; unrelated description URLs are not rewritten.

## Updates and confirmations

**a** assigns, **s** changes status (t remains an alias), **e** edits, **c** comments, **L** links tickets, **m** reviews a Board/Backlog move. Every update targets one validated ticket and requires **y** confirmation; n/Esc cancels. Read-only mode blocks both entry and confirmation.

Forms use Tab, Ctrl+U to clear, Ctrl+S to review/save. The Markdown editor supports cursor/editing keys and bracketed paste. Description edits are **not lossless** for every ADF feature: mentions, embedded media and unsupported extensions may change. Review before confirming; use Jira web for those cases.

Membership moves require a complete verified scope and re-fetch membership after confirmation. Already-at-destination is a no-op; epics cannot be moved. No status transition is requested; Jira chooses destination Rank.

```sh
jirust move DEV-123 --to board
jirust move DEV-123 --to backlog
```

Replace the synthetic key with one intended ticket. CLI moves ask you to type the key; `--yes` is explicit single-ticket confirmation for noninteractive use. Read-only blocks them too. **Never automatically retry an uncertain write**: reload first. A server change between the final membership read and POST remains possible.

Successful writes fully refresh **all four views**, fence older reads, preserve filters/search and retain selection/reading position where possible. Failed writes or failed follow-up refreshes never repeat the mutation.

## Ticket fields and images

Press **?** for the in-app symbol legend at the top of Help; use ↑/↓ to scroll and Esc to close.

Issue-type icons appear beside ticket keys in both list layouts and beside the full type name in details: ● Bug, □ Task, ≡ Story, ✎ Design, ϟ Epic, ⊟ Sub-task, ↗ Improvement, ◆ Feature, ◇ Spike. Custom or missing types use ◈; the exact server type name remains in details. These are Unicode symbols; no Nerd Font is required.

Only explicit **parent epic labels** are colored—not epic rows/titles. Colors are distinct and persist per account/key across views and renames. Missing/non-epic parent relationships are not guessed. Done badges are green/✓, Discarded red/✕; other statuses have distinct readable colors. Styling never determines workflow or membership.

Absent people and unknown/ambiguous story points are blank. Genuine zero/fractions remain visible. Points come from numeric server-named/schema-identified fields, never guessed IDs or child sums. Full details/metadata permissions can affect availability; restart after fixing field access.

PNG/JPEG/GIF/WebP attachments appear as still images after the description, fill the detail width, preserve aspect ratio and scroll in slices. **i** jumps/retries, **l** or a caption opens a tab-wide preview, **I** selects a local file, Esc closes preview. ADF media UUIDs are not guessed into attachment IDs. External media, SVG and non-raster files are not automatically rendered.

Downloads require a numeric attachment ID verified on the selected ticket and use fixed pinned Cloud endpoints with no redirects/CDN credential forwarding. Redirected/unsupported content should be downloaded manually through Jira. No attachment upload/delete is implemented. Local files may be placed at `<attachment-dir>/<ticket-key>/<filename>`; traversal is rejected.

Use `--no-images` to disable automatic loading, `--image-protocol halfblocks` for a portable fallback, or kitty where supported. Downloads are bounded at 16 MiB/12 million pixels; scaled rasters/decoder allocation/concurrency/private image caches have separate limits. See [security/resource boundaries](../SECURITY.md), including best-effort persistence and encoded budgets rather than exact RSS guarantees.

## Refresh and diagnostics

**F5 / Ctrl+R / browsing r** fully refreshes membership, Rank, Mine and scoped details while keeping cached data visible. Progress shows phases/time. Filters, sort, selection and scroll survive where possible; stale reads cannot replace newer data. Repeated requests coalesce; writes block manual refresh. Modal/editor controls retain their own keys.

Periodic sync defaults to 60 seconds, with configurable refresh/retry intervals. Only selected/nearby full details are warmed (six candidates, two background reads); obsolete reads are cancelled, never mutations. Cache writes run on a bounded serialized worker and drain after terminal restoration on graceful quit. Private account-scoped caches are best-effort; `--no-cache` disables persistence.

Developer diagnostics from the checkout:

```sh
cargo run --release --example search_bench -- --epic
cargo run --release --example refresh_check
cargo run --release --example image_check -- DEV-123
```

Search benchmarking is synthetic/account-free. Refresh/image checks use existing authorized configuration, are read-only, and do not save settings/cache. For the image check, explicitly choose one permitted live ticket instead of the synthetic example key. These checks do not establish universal Jira or terminal/macOS compatibility.
