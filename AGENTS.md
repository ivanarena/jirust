# Project guidelines

- This is a personal, private project. Do not change repository visibility or move it to an organization.
- The UI delegates to existing `acli`/`jira-cli` commands. Do not introduce a direct Jira HTTP client or extract/reimplement CLI authentication without explicit approval.
- Use `tokio::process::Command` with argument arrays, never shell interpolation, for Jira operations.
- Never commit credentials, CLI account configuration, caches, ticket bodies, screenshots, or fixtures from real accounts. Tests use synthetic DEV tickets only.
- Never perform real Jira mutations as part of development/tests. Every user-facing write must target one validated ticket key and require explicit confirmation. Respect `--read-only` in both action entry and confirmation.
- Do not describe a status-grouped board or backlog JQL as an exact replica of a web board. Document unsupported CLI capabilities.
- Cache local ticket data only in account-scoped namespaces with private file permissions. Keep terminal control sequences out of untrusted Jira text and validate external URLs.
- Run `make check` and `make build` before committing. Keep backend, UI, and cache/config code separated.
