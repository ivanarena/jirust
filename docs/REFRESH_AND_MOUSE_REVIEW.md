# Refresh and semantic mouse review — 0.16.0

## Scope

Local implementation review, synthetic regression tests and authorized read-only timings. No real Jira mutations, publication, credential changes or relaxed API/identity boundaries. Native macOS CI is not a live macOS terminal/Jira validation.

## Changes and bug fixes

- Skip redundant board-discovery subprocesses for API-token transport; OAuth still refreshes through acli. Every approved HTTP request retains pinned-profile/site validation and existing bounds.
- Complete shared membership/Rank/Mine refresh remains mandatory. Demand-load full details for the selection and at most six nearby candidates, with two background reads. Cancel obsolete background reads on full refresh/scope/view changes; fence cancelled completions so they cannot clear a newer pending request. Never cancel or retry a mutation.
- Use the existing key index for old/new snapshot comparison. Copy only persisted workspace fields into cache jobs, excluding the derived index; avoid large cache snapshot copies when persistence is disabled.
- Serialize/encode/lock/quota-check/write/remove caches on a dedicated coalesced worker. Ordered invalidation prevents an old queued write from resurrecting removed ticket data. Snapshot pressure cannot evict removals or palette metadata; pending snapshot classes/counts and metadata encoding are bounded. Existing private-filesystem checks remain intact. Persistence is best-effort, not an exact RSS guarantee or a crash-proof delivery contract.
- Generation-fenced phase/elapsed progress remains visible during local navigation/filtering; late progress cannot overwrite a new refresh. Restore the terminal before draining accepted cache jobs at graceful quit.
- Semantic double-click filters use exact assignee identities, full status names and explicit epic keys. Duplicate display names and identical epic labels never merge identities. Other field/text filters, current-view presets/membership and local sort stay intact.
- Compute click targets from rendered Unicode cells, clipped labels, the final table scroll offset and typed detail chunks. Metadata is literal, not re-parsed user Markdown; ordinary body words/headers/dialogs cannot become accidental field-filter targets. Assignee/status metadata labels also work for blank/unassigned values.
- Defer a single detail-epic link for 350 ms so its first click does not navigate away before the second click can filter. Other input/navigation cancels deferred activation. Double-click requires the same field, ticket, view, pane zone, nearby x coordinate and identical y within the window.
- Preserve typed text search when clicking a row before applying its field filter. Clear Epic independently with v; F6 clears all current-view filters. Runtime filters remain per-view and do not persist across restart.

## Evidence

Synthetic tests cover wide/narrow rows, Unicode/clipping/scroll offsets, duplicate identities, key-specific epic filtering, literal metadata versus body text, detail-link deferral, clipboard side effects, modal/read-only boundaries, independent view filters/sort, complete-refresh preservation, bounded demand-prefetch, generation fencing, child-process read cancellation, worker-thread execution, snapshot pressure/coalescing/invalidation and existing safety/resource regressions.

Read-only live full workspace timing: first pass 4.47 s; subsequent passes 3.44 s and 3.17 s. This excludes startup configuration and selected full-detail loading, and is not a universal latency guarantee. Previously measured redundant CLI discovery alone: median 0.82 s. A synthetic 1,000-ticket Epic-sort search benchmark measured median indexed input 1.23 ms (worst 1.92 ms); host/network load varies. No arbitrary new HTTP endpoints or attachment downloads were introduced by this diagnostic.
