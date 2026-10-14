# Security policy

Jirust is experimental software, not a hosted service. Only the latest released version receives security fixes; use read-only mode until you have reviewed its behavior for your Jira configuration. Development tests never mutate a real Jira account.

## Reporting

The repository is currently private. Authorized collaborators should report concerns through a **private repository issue** or an existing private channel with the owner. Do not include tokens, real ticket bodies, CLI configuration, dotenv files, clipboard contents or unredacted logs. Describe reproduction steps using synthetic DEV tickets.

**Before any public publication**, the owner must enable GitHub Private Vulnerability Reporting and verify that [Report a vulnerability](https://github.com/ivanarena/jirust/security/advisories/new) is available. Once enabled, use that private channel, not public issues, for potential vulnerabilities. Do not change visibility merely to enable reporting. Response is best-effort; there is no guaranteed response-time SLA.

## Boundaries

- Ordinary Jira operations use argument-array CLI subprocesses. Confirmed writes target one validated key, respect read-only mode and are never automatically retried. TUI and CLI membership moves re-fetch scope/membership after confirmation; already-at-destination is a no-op. Jira does not offer an atomic client-side lock between that read and the POST, so concurrent server changes remain possible.
- Direct Jira HTTP is limited to the approved account/site-pinned Board/Backlog bridge, confirmed single-ticket membership moves, verified numeric raster attachment downloads and read-only field metadata. No authenticated redirects, external-media fetching, attachment mutation or automatic OAuth refresh are implemented.
- Credentials are never placed in argv, displayed through CLI diagnostics or exported from dotenv into child environments. CLI stderr is drained under a byte limit but not retained. Failed-write receipts and JSON decode failures also suppress raw content.
- Unix application storage uses owned private directories/files, no-follow leaf opens, regular-file/hardlink checks, bounded reads, atomic directory-anchored writes and nonblocking writer locks. The configured parent hierarchy is trusted: this is not a sandbox against a malicious process running as the same OS user or a compromised CLI/terminal/browser. Explicit local images may come from user-selected ordinary readable files.
- Encoded-data budgets: CLI stdout 32 MiB / stderr 256 KiB; ticket cache entry 64 MiB; workspace 64 MiB / 20,000 tickets; each account namespace's managed JSON cache 256 MiB / 4,096 entries. Limits cause an error, not a partial authoritative snapshot. Cache writes are best-effort; quota/full-disk/lock failures preserve existing cache entries while the live in-memory view can continue. Images retain separate download/decoder/128 MiB cache limits. These are encoded-data/count bounds, not an exact RSS guarantee or a global quota across accounts.
- Regular cache/config leaf symlinks, unsafe permissions and unexpected ownership are rejected. Newly created private directories are 0700 and files 0600. Existing owned directory permissions may be tightened. Platform-parent symlinks (for example macOS temporary-directory ancestors) are not blanket-rejected.
- Jira-derived text is stripped of terminal controls. Clipboard writes are explicit row-click behavior; `--no-copy-on-click` disables them. The application never reads clipboard contents. OSC52 requires a trusted terminal/multiplexer.

## Continuous checks

Pinned GitHub Actions run native Linux/macOS tests and a Rust 1.90 minimum-version job. A credential-free security job runs on pushes, PRs and weekly: checksum-verified Gitleaks history/tree scans, RustSec auditing with maintenance warnings denied, and reproducible third-party notice/source-inventory checks. No Jira account is used in CI.

A clean scanner result is not a security guarantee. Audit the exact proposed release revision and packaging output before distributing it. The [support matrix](docs/SUPPORT.md) distinguishes native CI from live integration coverage.
