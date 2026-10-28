# Changelog

## 0.17.4

- Available now requires both an unassigned ticket and the exact status **To Do**, in local cached views and generated JQL. In Progress, Done and other statuses stay excluded even when filters are cleared or changed.
- Update the demo and help text to match; Board/Backlog membership and Mine remain unchanged.

## 0.17.3

First tagged release. Jirust remains experimental.

### Changes since 0.17.2

- Refresh all shared views after confirmed writes, updating local rows, details and search immediately while rejecting stale pre-write reads. Failed writes are never automatically retried.
- Handle stale Board membership IDs during refresh without guessing replacement membership.
- Fetch all configured project statuses, including unused workflow states, and preserve cached status suggestions and history across refreshes and overlapping writes.
- Add issue-type icons and an in-app symbol legend.
- Update vulnerable dependencies and regenerate third-party notices and source inventory.

### Support limits

- Jira Cloud on Linux/macOS; Windows is not natively supported.
- Exact Board/Backlog membership is limited to recognized team-managed manual `ISSUE_LIST` metadata. Other board strategies are not established.
- Another authorized Jira site/board and live macOS Jira/terminal/clipboard/image behavior remain unverified. Native CI is not live integration validation.
- Start with `--read-only`; ticket updates require explicit confirmation.

See [the support matrix](docs/SUPPORT.md) and [security policy](SECURITY.md) for the full boundaries.
