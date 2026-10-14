# Public-readiness audit

Date: 2026-10-13

Audited revision: `54382fc1d7c16eddd9ab894dc3dfa9fc6f5244d3` (Jirust 0.13.2)

## Remediation review — 0.14.0

The initial audit below is retained as historical evidence for 0.13.2. The locally reproducible findings have now been remediated and reviewed in 0.14.0:

| Finding | Remediation / verification |
|---|---|
| M1 | TUI has a dedicated confirmed Move job with captured scope/Board/read-only context and uses the shared fresh-state CLI execution path. Synthetic scope/Board drift, no-op-at-destination, read-only and single-move-count tests pass. |
| M2 | Raw stderr is bounded and discarded; failure receipts and JSON decode errors suppress content. The original synthetic credential-leak reproduction now returns **false**. |
| M3 | Bounded streamed subprocess reads with overflow termination, bounded serialization/cache reads, count/aggregate workspace budgets (including detail updates), JSON cache quotas and writer locks. Overflow never produces a silently complete partial scope. |
| M4 | serde_yaml/unsafe-libyaml removed; maintained serde-saphyr with input/nesting/alias budgets, unsupported-tag rejection and no file inclusion. Native/standard/OAuth/quoted-ID and malformed-profile tests pass. |
| L1 | Owned private regular files/directories, no-follow leaf opens, hardlink/symlink checks, atomic directory-handle writes and safe migration checks. Image cache reads share the safer reader. The original nonprivate symlink-cache reproduction now returns **false**. |
| L2 | Immutable Action revisions, Linux/macOS/stable plus Rust 1.90 CI, weekly history/source secret scans, maintenance-warning-denying dependency audit, deterministic license/source notices, SECURITY/support/release documentation and local-only packaging. Public private-reporting activation remains an explicit owner publication prerequisite while this repository is private. |

Local evidence: `make check` (**161 library + 5 binary = 166 tests**), repeated regression runs, release build, credential-free demo and existing-site read-only/no-cache diagnostic passed. RustSec checked **302 locked packages: 0 vulnerabilities / 0 warnings**. A proposed-publication-tree scan found no organization/credential markers or Gitleaks findings. Exact sources/checksums are inventoried; MPL covered source is included unchanged by the local packaging script.

The final code review also addressed async-buffer stack growth, short fork/exec lock-inheritance windows, aggregate-budget drift after Mine hydration, and demo attachment defaults that could otherwise touch personal cache directories. Cache persistence remains best-effort under quota/lock/full-disk failures, and encoded-data budgets are not an exact RSS guarantee. Configured parent directories and same-user CLI/terminal processes remain trusted, not sandboxed.

Committed-runtime CI **38534972414** passed for **`ad74e7f36cf978e23cf1ef0cc4e4f3aef2938b9b`**: Linux stable, macOS stable, Rust 1.90 and the credential-free secret/dependency/notices job. A local bundle from **`c2f7618c683c8769efce161325b0a55106937427`** (same runtime plus notice-format attributes) passed archive checksum, binary identity, exact committed source, MPL archive checksum, organization-marker and Gitleaks archive checks. No bundle was uploaded. Full-history Gitleaks and Git integrity checks also passed after the runtime commit.

**Remaining external evidence:** another authorized live Jira site/board and live macOS terminal/clipboard/image behavior. These were not available and are not marked passed. Recheck all gates for any later source changes; public private-reporting activation and publication still require the owner's explicit decision. Repository visibility is unchanged; no real Jira mutations were performed.

**Assessment:** locally fixed and ready for a narrowly documented experimental-beta preparation, not a universal compatibility or independent penetration-test sign-off. See [SUPPORT.md](SUPPORT.md), [SECURITY.md](../SECURITY.md) and [RELEASING.md](RELEASING.md).

## Initial 0.13.2 decision

**Privacy/history: green. Public-release hardening: amber (at the initial revision).**

No real credentials, company identifiers, real company ticket keys, account configuration, ticket-data fixtures or screenshots were detected in the audited Git history or GitHub Actions logs. No history rewrite or credential rotation is indicated by these results. This is evidence from the checks below, not a guarantee that every possible secret representation has been detected.

The core is configurable rather than company-hardcoded. It is a credible open-source beta candidate, but the write-safety and diagnostic/resource findings below should be fixed before broadly distributing a write-enabled build. Do not advertise universal Jira/board/platform compatibility.

The repository remains private. No visibility changes, history rewrites, application restarts or real Jira mutations were performed during this audit. Runtime source was not changed; this document records findings, not remediations.

## Scope and evidence

- All local reachable refs and the remote branch/tag inventory: one branch (`main`), no tags. `git fsck --full --no-reflogs` passed.
- All **28 commits**, including the original Go prototype: **257 unique blob versions / 47 historical paths** inspected by a custom content/artifact/metadata scan.
- **Gitleaks 8.30.1**, official release archive verified against its published SHA-256 checksum: complete-history scan (`git --log-opts=--all`) and working-tree scan both reported **0 findings**.
- Custom scans checked known organization markers, ticket-key prefixes, cloud-site literals, credential-bearing URLs, private-key headers, token formats, JWT-shaped literals and sensitive artifact paths. **32 candidates** were manually classified as synthetic placeholder/rejection-test fixtures; **0 confirmed leaks** remained. Commit author/committer emails were GitHub noreply addresses; no organization markers appeared in commit metadata.
- All **28 GitHub Actions run logs** were downloaded privately and checked with Gitleaks plus the organization-marker scan: **0 findings**. All three historical workflow definitions were inspected; none used `pull_request_target` or referenced repository secrets. GitHub reported **0 releases, 0 Actions artifacts and 0 issue/PR entries**.
- **cargo-audit 0.22.2** checked **296 locked packages** against RustSec database commit `64a0302dc836df15a10ac27ebdcdf8f9b4ce845a` (updated 2026-10-13): **0 known vulnerabilities**, **1 unmaintained dependency warning** (below).
- `make check` passed: **148 library + 4 binary tests**. The credential-free, read-only demo diagnostic passed. Existing Linux/macOS CI for the audited revision passed.
- License metadata inventoried for all 296 resolved packages; none lacked license metadata. This inventory includes target-specific packages, not just the native Linux binary.
- Two runtime hardening gaps were reproduced with a temporary synthetic-only harness: credential-shaped CLI stderr survives into the application error; a nonprivate, symlinked ticket-cache entry is accepted. No real profile, credentials, ticket bodies or clipboard contents were used in those reproductions.

Raw scanner logs/reports and synthetic proof tooling were kept outside the repository in a private temporary directory. They should not be bundled with a release. This audit did not inspect unrelated private configuration/cache contents or change the live Jira account.

## Findings

### M1 — TUI membership moves bypass post-confirmation revalidation

**Priority: fix before a public write-enabled release.**

The command-line move flow re-fetches membership/scope after confirmation in [movement.rs](../src/movement.rs). The TUI instead validates its cached snapshot when creating the confirmation, then dispatches `Job::Write` directly to `Backend::apply` in [main.rs](../src/main.rs). For `board`/`backlog`, [backend.rs](../src/backend.rs) checks identity, key and Board ID, but calls `BoardApi::move_ticket`, which immediately posts in [board_api.rs](../src/board_api.rs).

A collaborator or background change can make the original membership/scope assumptions stale while the confirmation is open. A ticket already at the destination could still receive another move/rank-changing POST. Single-key validation and explicit confirmation remain present; this is a missing fresh-state check, not evidence of arbitrary-key injection.

**Recommendation:** route TUI moves through the same post-confirmation `movement::execute` path as the CLI, preserving read-only checks and the no-op-at-destination rule. Add synthetic tests for membership/scope changes during confirmation. Never retry an uncertain POST.

### M2 — Ordinary CLI diagnostic errors are not secret-redacted

**Priority: fix before public release.**

[backend.rs:207–230](../src/backend.rs#L207) forwards CLI stderr through `data::safe`. That strips terminal controls, but does not redact credentials. A synthetic helper emitting `Authorization: Bearer <synthetic token>` confirmed that the token-shaped text reaches the returned application error, which the UI can display.

Authentication subprocess output and direct HTTP errors are already suppressed/redacted. No real credential leak was observed. This finding concerns the ordinary CLI error path if an upstream CLI/debug/error message includes sensitive material.

**Recommendation:** provide generic errors by default and bounded, redacted diagnostics. At minimum redact authorization headers, credential-bearing URLs, recognizable tokens and configured secret values; do not retain or expose an unredacted fallback through the error viewer.

### M3 — CLI output and ordinary ticket-cache reads lack byte limits

**Priority: harden before public release.**

[backend.rs:223](../src/backend.rs#L223) uses `Command::output()` to collect stdout/stderr without byte caps. The 45-second timeout and concurrency limit do not bound memory. [config.rs:259](../src/config.rs#L259) reads an entire ticket-cache file before JSON decoding, also without a size check. Image and direct HTTP paths have explicit limits, but ordinary CLI text/JSON does not have equivalent limits. Aggregate workspace/text-cache volume should also be considered for large projects.

**Recommendation:** stream subprocess output with explicit limits and kill-on-overflow; validate cache entry sizes before reading; define an aggregate workspace/cache budget. Test large synthetic outputs and files. No actual memory-exhaustion attack was run.

### M4 — YAML dependency maintenance warning

**Priority: dependency maintenance.**

RustSec reports **RUSTSEC-2024-0449: `unsafe-libyaml` is unmaintained**. The dependency path is `jirust → serde_yaml 0.9.34+deprecated → unsafe-libyaml 0.2.11`.

This is **not a reported vulnerability** in the audited lockfile. YAML is used for local CLI profile metadata, which reduces its exposure compared with remote ticket parsing, but leaves future parser fixes dependent on an unmaintained component.

**Recommendation:** choose a maintained YAML parser/Serde integration and test all supported CLI profile layouts before replacing it. Add recurring dependency auditing to CI.

### L1 — Ticket-cache read protections are weaker than image/env protections

**Priority: defense in depth.**

[config.rs](../src/config.rs) does not reject symlinked or nonprivate ticket-cache entries on read; `private_dir` also follows an existing directory symlink. A synthetic 0644 file reached through a cache-entry symlink was accepted. Environment and image-cache readers already reject relevant symlinks/nonprivate files.

Normal cache directories are private, so this is principally local same-user/shared-directory tampering or unsafe migration/configuration, not a demonstrated cross-user privilege escalation.

**Recommendation:** validate owned regular files/directories, permissions and symlinks consistently; avoid check/open races where practical. Include cache migration in those tests.

### L2 — Public CI/security/release policy is incomplete

CI is credential-free and uses read-only repository permissions, which is good. Actions are referenced by mutable tags/branches (`checkout@v4`, `rust-toolchain@stable`, `rust-cache@v2`), rather than immutable commit SHAs. There is no `SECURITY.md`, ongoing secret/dependency scan, declared vulnerability-reporting process, or minimum-Rust-version CI job. Native CI does not prove live Jira/terminal parity on macOS.

**Recommendation:** pin Actions, add automated audits and minimum-version coverage, and establish an owner-approved private reporting channel before publication. Add release/checksum instructions and a clear supported-platform/board/auth matrix.

## Compatibility and licensing

- Company URL/email/project/Board/token are runtime configuration; workflow choices are discovered rather than a fixed company workflow. Herdr is optional.
- The exact membership bridge relies on version-sensitive Jira Cloud GreenHopper metadata and the supported team-managed `ISSUE_LIST` strategy. Unsupported strategies fail closed; ordinary CLI functionality is broader, but does not establish universal Board/Backlog parity.
- Live use is primarily Jira Cloud/acli. The jira-cli adapter is limited/partial; Windows and Jira Data Center compatibility are not established. Parent-epic display requires explicit parent type metadata rather than guessed classic custom-field IDs.
- Authentication, CLI installation, image protocol and terminal clipboard support still need clear onboarding. Click-to-copy is documented, but an opt-out would make the default less opinionated for new users.
- The project is **MIT licensed**. Most dependencies offer permissive licensing. `option-ext 0.2.0` (via `directories`) is **MPL-2.0**: assess covered-source availability and notice obligations when distributing binaries. Alternative GPL/LGPL expressions elsewhere have permissive options; they do not by themselves force the entire project under GPL/LGPL. Unicode/CDLA/other notices also need consideration for binary packaging. Add third-party license/source notices to releases; metadata review is not legal advice.
- No prebuilt GitHub release artifacts currently exist, so this audit does not sign off on a binary-distribution bundle.

## Suggested next pass

1. Fix M1/M2/M3, with synthetic regression tests and no real Jira mutations.
2. Replace the unmaintained YAML dependency and harden cache reads.
3. Add public security/support documentation, pinned audit-enabled CI and release notices.
4. Validate read-only behavior on a second Jira site/board and representative macOS terminals with authorized accounts.
5. Re-run the history/working-tree/Actions/dependency checks against the exact proposed release revision. Keep visibility unchanged until the owner explicitly approves publication.
