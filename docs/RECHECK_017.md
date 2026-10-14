# 0.17 configuration and regression recheck

Reviewed baseline: 0.17.0 (`a8e2f73`); remediation: **0.17.1**. This is a maintainer recheck, not an independent penetration test or universal compatibility certification. No real Jira mutation, authentication change, live-session restart or publication was performed.

## Findings reproduced and fixed

1. **Default dotenv discovery swallowed inspection failures.** An unreadable default path could be treated as absent and trigger fallback to another file. A synthetic blocked-directory test reproduced the fallback. Discovery now distinguishes an absent file from permission/I/O failures and fails closed on inspection errors, without resolving another profile.
2. **Saved-settings schema errors could echo content.** Wrong-type numeric settings produced serde errors containing the offending string. Synthetic sentinel values reproduced the leak. Schema-conversion errors now suppress all raw values, just like syntax errors. Regression coverage includes view, layout and board ID fields; legacy settings migration still passes.

These are defensive configuration fixes. No actual leaked credential was found, and the synthetic proofs do not imply that stored settings normally contain tokens.

## Completed local checks

- Formatting, all-target Clippy with warnings denied, debug regression tests and release build.
- **195 tests** (187 library + 8 binary) pass in debug and optimized release profiles; three additional all-target debug runs pass. Example targets compile/test with the debug all-target gate.
- Private dotenv file/profile precedence, explicit/relative selectors, missing-file rejection, unsafe URL/error redaction, positive image/clipboard overrides, and personal-environment isolation in demo.
- Wide/narrow synthetic PTYs: startup scope/view, rendering, sort, refresh, clear, dialogs and clean quit. No clipboard writes or personal-cache access.
- Installed/build binary identity checked at baseline; remediation installation is gated on native CI. No in-place live-process restart.
- Live authorized **read-only, no-cache** workspace/detail diagnostic passes. No ticket keys, bodies or credentials were stored in this report; no live image/clipboard operation was performed in this recheck.
- Browser aliases reviewed separately from API origins: runtime-only ticket decoration, cache serialization unchanged, source site/account/cache identity retained, fixed approved HTTP endpoints and no redirects unchanged.
- Read-only/confirmation/single-ticket move revalidation, full postwrite refresh fences, per-view filters/Epic ordering, cache worker ordering and private filesystem/process limits reviewed; existing regression coverage passes.
- Credential/config files remain 0600 and private application directories 0700; contents were not printed.
- Git integrity passes. Gitleaks full reachable history: **0 findings**. Exact locally configured token/site/email/ticket-key patterns checked across **334 historical blobs / 34 baseline commits**, all **0 matches**, without printing/exporting those values.
- Latest 0.16/0.17 CI logs downloaded privately and scanned: **0 findings**. Proposed tracked-tree and remediation-history scans are release gates repeated before/after commit.
- RustSec: **0 vulnerabilities / 0 warnings**. Deterministic third-party notices/source inventory regenerated for the version-only lockfile change and checked; no dependencies added.
- Baseline native Linux/macOS, Rust 1.90 and security CI all green. The remediation commit must pass the same jobs before installation; final run status is checked through GitHub rather than assumed from local tests.

## Distribution and remaining boundaries

The exact clean remediation revision may be packaged **locally only** after its gates pass. Verify binary/source identity, archive safety, excluded private files, license/source inventory and covered-source checksums. Packaging does not authorize upload or visibility changes.

Repository remains **personal/private**. Experimental-beta readiness is not universal Jira support. A second live authorized site/board, live macOS Jira/graphics/clipboard parity and other board strategies remain unverified. Before any owner-approved public publication, verify private vulnerability reporting, update private-repository wording and review the exact distribution's notices/source obligations. See `SECURITY.md`, `SUPPORT.md`, `RELEASING.md` and `CONFIGURATION.md`.
