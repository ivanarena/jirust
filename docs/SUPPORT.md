# Support and validation matrix

Jirust is an **experimental Jira Cloud terminal client**. Configuration is not company-hardcoded, but supported authentication, Board metadata and terminal behavior are intentionally bounded.

| Area | Evidence / supported boundary |
|---|---|
| Linux | Native build/tests plus live read-only Jira Cloud/acli use; Kitty-capable hosting and Wayland clipboard checked. |
| macOS | Native CI build/tests; explicit private dotenv token setup supported. Live Jira and graphics/clipboard parity remain **unverified**. |
| Rust | Minimum 1.90, checked separately in CI; stable native Linux/macOS jobs. |
| Windows | No native support claim; private filesystem primitives intentionally require Unix. WSL follows Linux behavior, but is not separately validated. |
| Jira Cloud/acli | Primary integration. Existing authentication must match the configured site/account. OAuth refresh stays with acli. |
| jira-cli | Partial adapter, capped at 100 rows; does not provide the exact membership/API bridge. |
| Jira Data Center / government Cloud | Not established; direct bridges require a supported `.atlassian.net` Cloud origin. |
| Exact Board/Backlog | Team-managed manual `ISSUE_LIST` strategy with recognized GreenHopper metadata and nontruncated, disjoint membership. Unsupported schema/strategies fail closed. This is version-sensitive, not a universal public Jira API guarantee. |
| Classic Scrum/Kanban / other strategies | Exact membership parity not established. Do not infer membership from statuses or sprint fields. |
| Multiple sites | Account/site changes are pinned/rejected; synthetic identity-conflict coverage exists. A second authorized live site is not yet validated. |
| Story points / parent epic | Numeric server-named point fields and explicit parent Epic type/hierarchy. Ambiguous/localized/custom naming can leave points blank; classic custom-field-only epic relationships are not guessed. |
| Images | Verified raster attachments only; Kitty or Unicode fallback. No universal graphics claim across terminals/multiplexers. |
| Clipboard | Native stdin-only helpers or terminal OSC52. Host configuration matters; use `--no-copy-on-click` to disable row-click copies. |

## Read-only validation checklist

Use a separately authorized test account/site; do not copy another person's credential files. Obtain the token privately, authenticate explicitly, and never post account data in issues or fixture commits.

1. Install the documented CLI, Rust and terminal dependencies. Use a 0600 private dotenv file and matching CLI profile.
2. Run `jirust --read-only --no-cache --check`. This performs reads only; it prints counts, not ticket bodies/keys or credentials.
3. Launch `jirust --read-only --no-cache`, select the intended scope/Board and compare membership and Rank with Jira web. Exercise all four views and independent filters. Unsupported schemas must retain useful cached data or show an explicit error, not guess a replacement Board.
4. Verify ordinary and epic tickets, blank unset fields, explicit parent labels, status colors, Unicode/narrow rendering and search. Do not change Jira records to manufacture fixtures.
5. Verify a permitted raster attachment and scrolling/fallback behavior. Test row-click clipboard writes only with deliberate clicks; verify opt-out. Never capture or share real ticket screenshots/logs for CI.
6. Check account-switch rejection using intentionally separate profiles, without silently authenticating or modifying the other account. Restore the intended account explicitly before normal use.
7. Record only sanitized pass/fail/platform/CLI/terminal versions in a review. Do not claim live compatibility from a hosted CI test.

**Outstanding external validation:** another authorized Jira site/board, and live macOS terminal/clipboard/image behavior. No such account or native terminal was available during remediation; these checks must not be marked complete without evidence.
